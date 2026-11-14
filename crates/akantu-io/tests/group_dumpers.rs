// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the default
// group dumpers and group field factories (MESH-022, MESH-023,
// MESH-027; upstream src/mesh/element_group.cc, node_group.cc,
// group_manager_inline_impl.hh, mesh.cc getGroupDumper).
//
// `group_dumpers_replay_the_dump_probe_through_mesh_dumpers` replays the
// oracle case fixtures/dump/groups (oracle/probes/dump/dump_probe.cc,
// `case_groups`: `grp.dump()` / `ngrp.dump()` use the dumpers the
// ElementGroup / NodeGroup constructors register, the fields come from
// `mesh.createNodalField` / `createStridedNodalField` /
// `createElementalField`) through `MeshDumpers` and the factories, and
// compares the files byte for byte.

mod dump_common;

use akantu_core::Element;
use akantu_core::{ElementKind, ElementType, Error, GhostType, Result};
use akantu_io::dumper::group::{
    ALL_GROUP, MeshDumpers, SharedMeshDumpers, create_elemental_field,
    create_field_from_attached_data, create_nodal_field, create_strided_nodal_field,
    element_group_dumpable, group_dumper_name, node_group_dumpable, rename_group_in,
};
use akantu_io::{
    DumpBuffer, DumperParaview, FieldDescriptor, FieldResolver, FieldSource, FieldSupport,
    FieldValueType, MeshFilter, MeshSelection, Padding,
};
use akantu_mesh::{Mesh, SharedMesh};
use dump_common::{Inputs, add_tag_data, assert_same_tree, fixtures, key, m2_mesh, scratch};

const NG: GhostType = GhostType::NotGhost;

fn model(name: &str) -> FieldSource {
    FieldSource::ModelNodal(name.into())
}

fn model_el(name: &str) -> FieldSource {
    FieldSource::ModelElemental(name.into())
}

/// Resolver serving the dump probe's M2 arrays of the current stage (as
/// `dump_oracle.rs`).
struct M2 {
    inputs: Inputs,
    stage: usize,
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
                    "disp" => DumpBuffer::Real {
                        nb_component,
                        values: i.reals(&k),
                    },
                    "blocked" => DumpBuffer::Bool {
                        nb_component,
                        values: i.bools(&k),
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
}

/// The M2 mesh of the dump probe with the element group `grp` and the node
/// group `ngrp` of `case_groups`.
fn groups_mesh(inputs: &Inputs) -> Mesh {
    let mut mesh = m2_mesh(true);
    let mut g = mesh.groups_mut().create_element_group("grp", 2).unwrap();
    for (ty, k) in [
        (ElementType::Triangle3, "grp_tri3"),
        (ElementType::Quadrangle4, "grp_quad4"),
    ] {
        let ids = inputs.ints(k);
        let mut a = g.elements.alloc(ids.len(), 1, ty, NG, 0).unwrap();
        a.as_mut_slice().copy_from_slice(&ids);
    }
    g.node_group.set_nodes(inputs.ints("grp_nodes"));
    let ng = mesh.groups_mut().create_node_group("ngrp").unwrap();
    ng.set_nodes(inputs.ints("ngrp_nodes"));
    mesh
}

#[test]
fn group_dumpers_replay_the_dump_probe_through_mesh_dumpers() {
    let dir = scratch("group_dumpers");
    let pv = dir.join("paraview");
    let mut r = M2 {
        inputs: Inputs::load("groups"),
        stage: 0,
    };
    let mesh = groups_mesh(&r.inputs);
    let mut dumpers = MeshDumpers::new();
    let real = FieldValueType::Real;
    let fields = [
        create_nodal_field(&mesh, "disp", model("disp"), real, "grp", 3).unwrap(),
        create_nodal_field(
            &mesh,
            "blocked",
            model("blocked"),
            FieldValueType::Bool,
            "grp",
            0,
        )
        .unwrap(),
        create_strided_nodal_field(&mesh, "disp_y", model("disp"), real, "grp", 1, 1, 0).unwrap(),
        create_elemental_field(
            &mesh,
            "ereal",
            model_el("ereal"),
            FieldSupport::Elemental,
            real,
            "grp",
            Some(2),
            ElementKind::Regular,
            false,
        )
        .unwrap(),
        create_elemental_field(
            &mesh,
            "eint",
            model_el("eint"),
            FieldSupport::Elemental,
            FieldValueType::Int,
            "grp",
            Some(2),
            ElementKind::Regular,
            false,
        )
        .unwrap(),
        // createElementalField<Real, InternalMaterialField> + homogenizer
        create_elemental_field(
            &mesh,
            "qp",
            FieldSource::Internal {
                law: None,
                name: "qp".into(),
            },
            FieldSupport::QuadraturePoint,
            real,
            "grp",
            Some(2),
            ElementKind::Regular,
            true,
        )
        .unwrap(),
    ];
    {
        let grp = dumpers.element_group(&mesh, "grp").unwrap();
        assert_eq!(grp.default_dumper_name(), Some("paraview_grp"));
        grp.set_directory(&pv).unwrap();
        for f in fields {
            grp.add_dump_field(f).unwrap();
        }
        grp.dump(&mesh, &r, None, None).unwrap();
    }
    r.stage = 1;
    {
        let grp = dumpers.element_group(&mesh, "grp").unwrap();
        grp.dump(&mesh, &r, None, None).unwrap();
        // second dumper on the group: registerDumper<DumperParaview>(name,
        // file) + addDumpFilteredMeshToDumper(..., _all_dimensions)
        grp.register_paraview("grp_ascii", Some("grp_ascii"), false);
        grp.set_directory_to_dumper("grp_ascii", &pv).unwrap();
        grp.add_dump_mesh_to_dumper(
            "grp_ascii",
            MeshSelection::new(None, NG, ElementKind::NotDefined)
                .filtered(MeshFilter::ElementGroup("grp".into())),
        )
        .unwrap();
        grp.set_text_mode_to_dumper("grp_ascii").unwrap();
        grp.add_dump_field_to_dumper(
            "grp_ascii",
            create_nodal_field(&mesh, "disp", model("disp"), real, "grp", 3).unwrap(),
        )
        .unwrap();
        grp.dump_named("grp_ascii", &mesh, &r, None, None).unwrap();
    }
    {
        let ngrp = dumpers.node_group(&mesh, "ngrp").unwrap();
        assert_eq!(ngrp.default_dumper_name(), Some("paraview_ngrp"));
        ngrp.set_directory(&pv).unwrap();
        ngrp.dump(&mesh, &r, None, None).unwrap();
        ngrp.set_text_mode().unwrap();
        ngrp.set_base_name("ngrp_ascii").unwrap();
        ngrp.dump(&mesh, &r, None, None).unwrap();
    }
    assert_same_tree(&pv, &fixtures().join("groups/paraview"));
}

#[test]
fn default_dumpers_and_group_dumper_lookup() {
    let inputs = Inputs::load("groups");
    let mesh = groups_mesh(&inputs);
    assert_eq!(group_dumper_name("Top"), "paraview_Top");
    let e = element_group_dumpable("grp");
    assert_eq!(e.default_dumper_name(), Some("paraview_grp"));
    assert_eq!(e.dumper_names().collect::<Vec<_>>(), vec!["paraview_grp"]);
    let n = node_group_dumpable("ngrp");
    assert_eq!(n.default_dumper_name(), Some("paraview_ngrp"));
    let mut d = MeshDumpers::new();
    // unknown groups are errors (upstream throws)
    assert!(d.element_group(&mesh, "nope").is_err());
    assert!(d.node_group(&mesh, "nope").is_err());
    // a node group alone is not an element group
    assert!(d.element_group(&mesh, "ngrp").is_err());
    // getGroupDumper: "all" is the mesh's dumper, else the group's
    assert!(d.group_dumper(&mesh, "paraview_grp", "grp").is_ok());
    assert!(d.group_dumper(&mesh, "paraview_grp", ALL_GROUP).is_err());
    d.mesh(&mesh).register_paraview("mesh_all", None, true);
    assert!(d.group_dumper(&mesh, "mesh_all", ALL_GROUP).is_ok());
    assert!(d.group_dumper(&mesh, "mesh_all", "grp").is_err());
}

/// UD-672 (category A): upstream
/// `Mesh::getGroupDumper(dumper, group)` evaluates
/// `element_groups[group]->getDumper(dumper)`; for an unknown group
/// `std::map::operator[]` inserts a null `unique_ptr` and dereferences it
/// (undefined behaviour; the null entry also breaks later iterations over
/// the groups). The port returns `NotFound` and leaves the groups as they
/// were.
#[test]
fn group_dumper_of_an_unknown_group_is_not_found_ud672() {
    let inputs = Inputs::load("groups");
    let mesh = groups_mesh(&inputs);
    let before: Vec<String> = mesh
        .groups()
        .element_groups()
        .map(|g| g.name().to_owned())
        .collect();
    let mut d = MeshDumpers::new();
    let e = d
        .group_dumper(&mesh, "paraview_nope", "nope")
        .map(|_| ())
        .unwrap_err();
    assert!(
        matches!(&e, Error::NotFound { kind, id } if *kind == "element group" && id == "nope"),
        "{e:?}"
    );
    // a node group of that name is not an element group either
    assert!(matches!(
        d.group_dumper(&mesh, "paraview_ngrp", "ngrp").map(|_| ()),
        Err(Error::NotFound { .. })
    ));
    let after: Vec<String> = mesh
        .groups()
        .element_groups()
        .map(|g| g.name().to_owned())
        .collect();
    assert_eq!(before, after, "no entry inserted");
    // a known group with an unknown dumper is an error too
    assert!(d.group_dumper(&mesh, "nope", "grp").is_err());
}

#[test]
fn factories_check_groups_dimensions_and_padding() {
    let inputs = Inputs::load("groups");
    let mut mesh = groups_mesh(&inputs);
    let real = FieldValueType::Real;
    // "all" → no group restriction
    let f = create_nodal_field(&mesh, "disp", model("disp"), real, ALL_GROUP, 3).unwrap();
    assert_eq!((f.group, f.padding), (None, Padding::To3D));
    // node groups are not accepted by the nodal factories (upstream
    // getElementGroup)
    assert!(create_nodal_field(&mesh, "disp", model("disp"), real, "ngrp", 0).is_err());
    assert!(create_nodal_field(&mesh, "disp", model("disp"), real, "grp", 2).is_err());
    assert!(create_strided_nodal_field(&mesh, "d", model("disp"), real, "grp", 0, 1, 0).is_err());
    let s = create_strided_nodal_field(&mesh, "d", model("disp"), real, "grp", 0, 0, 0).unwrap();
    assert_eq!(s.source, model("disp"));
    // group dimension 2 ≠ 1: upstream bare `throw;` (UD-051) → error
    let e = |dim: Option<usize>, group: &str| {
        create_elemental_field(
            &mesh,
            "ereal",
            model_el("ereal"),
            FieldSupport::Elemental,
            real,
            group,
            dim,
            ElementKind::Regular,
            false,
        )
    };
    assert!(e(Some(2), "grp").is_ok());
    assert!(e(Some(1), "grp").is_err());
    assert!(e(None, "grp").is_err());
    assert!(e(Some(1), ALL_GROUP).is_ok()); // no check on the whole mesh
    assert!(
        create_elemental_field(
            &mesh,
            "x",
            model("disp"),
            FieldSupport::Nodal,
            real,
            ALL_GROUP,
            Some(2),
            ElementKind::Regular,
            false
        )
        .is_err()
    );
    // attached data: absent / other type → None; Int tags → a quadrature
    // point field on the mesh data
    assert!(
        create_field_from_attached_data(
            &mesh,
            "tag_0",
            FieldValueType::Int,
            "grp",
            ElementKind::Regular
        )
        .unwrap()
        .is_none()
    );
    add_tag_data(&mut mesh);
    let t = create_field_from_attached_data(
        &mesh,
        "tag_0",
        FieldValueType::Int,
        "grp",
        ElementKind::Regular,
    )
    .unwrap()
    .unwrap();
    assert_eq!(t.source, FieldSource::Mesh("tag_0".into()));
    assert_eq!(t.support, FieldSupport::QuadraturePoint);
    assert_eq!(t.group.as_deref(), Some("grp"));
    assert!(
        create_field_from_attached_data(&mesh, "tag_0", real, "grp", ElementKind::Regular)
            .unwrap()
            .is_none()
    );
    assert!(
        create_field_from_attached_data(
            &mesh,
            "tag_0",
            FieldValueType::Bool,
            "grp",
            ElementKind::Regular
        )
        .is_err()
    );
}

/// Upstream `renameElementGroup` moves the group object with its dumpers:
/// the dumper keeps its name and file name and reads the renamed group;
/// fields restricted to the group follow. The mesh's
/// rename is the only entry point, the registry follows by itself.
#[test]
fn renamed_groups_keep_their_dumpers() {
    let dir = scratch("group_dumpers_rename");
    let inputs = Inputs::load("groups");
    let mut mesh = groups_mesh(&inputs);
    let r = M2 { inputs, stage: 0 };
    let mut d = MeshDumpers::new();
    let disp =
        create_nodal_field(&mesh, "disp", model("disp"), FieldValueType::Real, "grp", 3).unwrap();
    d.element_group(&mesh, "grp")
        .unwrap()
        .set_directory(&dir.join("before"))
        .unwrap();
    d.element_group(&mesh, "grp")
        .unwrap()
        .add_dump_field(disp.clone())
        .unwrap();
    d.element_group(&mesh, "grp")
        .unwrap()
        .dump(&mesh, &r, None, None)
        .unwrap();
    mesh.groups_mut()
        .rename_element_group("grp", "grp2")
        .unwrap();
    assert!(d.element_group(&mesh, "grp").is_err());
    let g = d.element_group(&mesh, "grp2").unwrap();
    assert_eq!(g.default_dumper_name(), Some("paraview_grp"));
    g.set_directory(&dir.join("after")).unwrap();
    g.dump(&mesh, &r, None, None).unwrap();
    let before = std::fs::read(dir.join("before/grp-VTU/grp_0000.vtu")).unwrap();
    let after = std::fs::read(dir.join("after/grp-VTU/grp_0001.vtu")).unwrap();
    assert_eq!(before, after);
    // a model-owned dumpable re-targeted explicitly
    let mut other = akantu_io::Dumpable::new();
    other.register_dumper("p", Box::new(DumperParaview::with_name("p")), true);
    other
        .add_dump_mesh(
            MeshSelection::new(None, NG, ElementKind::NotDefined)
                .filtered(MeshFilter::ElementGroup("grp".into())),
        )
        .unwrap();
    other.add_dump_field(disp).unwrap();
    rename_group_in(&mut other, true, "grp", "grp2");
    other.set_directory(&dir.join("other")).unwrap();
    other.dump(&mesh, &r, None, None).unwrap();
    assert_eq!(
        std::fs::read(dir.join("other/p-VTU/p_0000.vtu")).unwrap(),
        before,
        "same group content, same field"
    );
    // destroyed groups: their dumpers are dropped
    mesh.groups_mut()
        .destroy_element_group("grp2", false)
        .unwrap();
    assert!(d.element_group(&mesh, "grp2").is_err());
}

#[test]
fn node_group_rename_follows_point_sets() {
    let inputs = Inputs::load("groups");
    let mut mesh = groups_mesh(&inputs);
    let r = M2 { inputs, stage: 0 };
    let dir = scratch("group_dumpers_node_rename");
    let mut d = MeshDumpers::new();
    d.node_group(&mesh, "ngrp")
        .unwrap()
        .set_directory(&dir)
        .unwrap();
    mesh.groups_mut()
        .rename_node_group("ngrp", "ngrp2")
        .unwrap();
    let n = d.node_group(&mesh, "ngrp2").unwrap();
    assert_eq!(n.default_dumper_name(), Some("paraview_ngrp"));
    n.dump(&mesh, &r, None, None).unwrap();
    assert!(dir.join("ngrp-VTU/ngrp_0000.vtu").exists());
    assert!(d.node_group(&mesh, "ngrp").is_err());
}

struct NoFields;

impl FieldResolver for NoFields {
    fn resolve(
        &self,
        _: &Mesh,
        f: &FieldDescriptor,
        _: Option<ElementType>,
        _: GhostType,
    ) -> Result<DumpBuffer> {
        Err(Error::not_found("field", f.name.clone()))
    }
}

/// Two triangles, no group.
fn two_triangles() -> Mesh {
    let mut m = Mesh::new(2, "m").unwrap();
    let mut a = akantu_mesh::MeshAccessor::new(&mut m);
    for x in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
        a.push_node(&x).unwrap();
    }
    a.connectivity_mut(ElementType::Triangle3, NG)
        .push_tuple(&[0, 1, 2]);
    a.connectivity_mut(ElementType::Triangle3, NG)
        .push_tuple(&[0, 2, 3]);
    a.make_ready().unwrap();
    m
}

/// Element group `name` holding triangle `el` (and its nodes).
fn tri_group(m: &mut Mesh, name: &str, el: akantu_core::Idx) {
    let conn = m
        .connectivity(ElementType::Triangle3, NG)
        .unwrap()
        .tuple(usize::try_from(el).unwrap())
        .to_vec();
    let mut g = m.groups_mut().create_element_group(name, 2).unwrap();
    g.add(
        Element::new(ElementType::Triangle3, el, NG),
        Some(&conn),
        true,
    )
    .unwrap();
    g.optimize();
}

fn vtu_files(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|r| {
            r.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

/// A group
/// destroyed and re-created under the same name is a new group object; as
/// upstream (whose new `ElementGroup` registers a new `paraview_<name>`) it
/// gets fresh default dumpers: step 0, default directory, no inherited
/// fields — not the old series.
#[test]
fn recreated_group_starts_a_new_series() {
    let dir = scratch("group_dumpers_recreated");
    let mut m = two_triangles();
    tri_group(&mut m, "g", 0);
    let mut d = MeshDumpers::new();
    let g = d.element_group(&m, "g").unwrap();
    g.set_directory(&dir.join("old")).unwrap();
    g.add_dump_field(
        create_nodal_field(&m, "disp", model("disp"), FieldValueType::Real, "g", 0).unwrap(),
    )
    .unwrap();
    g.dump(&m, &NoFields, None, None).unwrap_err(); // the field cannot resolve
    g.remove_dump_field("disp").unwrap();
    g.dump(&m, &NoFields, None, None).unwrap();
    let first = std::fs::read(dir.join("old/g-VTU/g_0000.vtu")).unwrap();
    m.groups_mut().destroy_element_group("g", true).unwrap();
    tri_group(&mut m, "g", 1);
    let g = d.element_group(&m, "g").unwrap();
    // fresh defaults: no field of the old group, the default directory
    assert_eq!(g.dumper_names().collect::<Vec<_>>(), vec!["paraview_g"]);
    g.set_directory(&dir.join("new")).unwrap();
    g.dump(&m, &NoFields, None, None).unwrap();
    assert_eq!(vtu_files(&dir.join("old/g-VTU")), vec!["g_0000.vtu"]);
    assert_eq!(vtu_files(&dir.join("new/g-VTU")), vec!["g_0000.vtu"]);
    let second = std::fs::read(dir.join("new/g-VTU/g_0000.vtu")).unwrap();
    assert_ne!(first, second, "the new group holds the other triangle");
    // the same holds for node groups and for replaced groups
    m.groups_mut().create_node_group("n").unwrap().add(0, true);
    let n = d.node_group(&m, "n").unwrap();
    n.set_directory(&dir.join("nodes")).unwrap();
    n.dump(&m, &NoFields, None, None).unwrap();
    m.groups_mut()
        .create_node_group_replace("n", true)
        .unwrap()
        .add(3, true);
    let n = d.node_group(&m, "n").unwrap();
    n.set_directory(&dir.join("nodes2")).unwrap();
    n.dump(&m, &NoFields, None, None).unwrap();
    assert_eq!(vtu_files(&dir.join("nodes2/n-VTU")), vec!["n_0000.vtu"]);
    m.groups_mut()
        .create_element_group_replace("g", 2, true)
        .unwrap();
    assert_eq!(
        d.element_group(&m, "g").unwrap().dumper_names().count(),
        1,
        "replaced: fresh defaults"
    );
}

/// Renames are followed through the mesh only, including
/// swaps (applied simultaneously) and fields of the mesh's own dumpers
/// restricted to a renamed group.
#[test]
fn renames_are_followed_simultaneously_and_everywhere() {
    let dir = scratch("group_dumpers_swap");
    let mut m = two_triangles();
    tri_group(&mut m, "a", 0);
    tri_group(&mut m, "b", 1);
    let mut d = MeshDumpers::new();
    for g in ["a", "b"] {
        d.element_group(&m, g)
            .unwrap()
            .set_directory(&dir.join(g))
            .unwrap();
    }
    // a field of the mesh's own dumper restricted to group "a"
    let mesh_d = d.mesh(&m);
    mesh_d.register_paraview("all", None, true);
    mesh_d
        .add_dump_mesh(MeshSelection::new(Some(2), NG, ElementKind::Regular))
        .unwrap();
    mesh_d
        .add_dump_field(
            create_elemental_field(
                &m,
                "f",
                model_el("f"),
                FieldSupport::Elemental,
                FieldValueType::Real,
                "a",
                Some(2),
                ElementKind::Regular,
                false,
            )
            .unwrap(),
        )
        .unwrap();
    // swap a and b through a temporary name between two accesses
    let groups = m.groups_mut();
    groups.rename_element_group("a", "tmp").unwrap();
    groups.rename_element_group("b", "a").unwrap();
    groups.rename_element_group("tmp", "b").unwrap();
    // the group now named "b" is the old "a": its dumper (file "a") follows
    let gb = d.element_group(&m, "b").unwrap();
    assert_eq!(gb.default_dumper_name(), Some("paraview_a"));
    gb.dump(&m, &NoFields, None, None).unwrap();
    let ga = d.element_group(&m, "a").unwrap();
    assert_eq!(ga.default_dumper_name(), Some("paraview_b"));
    ga.dump(&m, &NoFields, None, None).unwrap();
    // the old "a" (triangle 0) was written by the dumper of file name "a":
    // compare with the dumps of a mesh whose groups were never renamed
    let mut r = two_triangles();
    tri_group(&mut r, "a", 0);
    tri_group(&mut r, "b", 1);
    let mut rd = MeshDumpers::new();
    for g in ["a", "b"] {
        let x = rd.element_group(&r, g).unwrap();
        x.set_directory(&dir.join("ref").join(g)).unwrap();
        x.dump(&r, &NoFields, None, None).unwrap();
    }
    for g in ["a", "b"] {
        let f = format!("{g}/{g}-VTU/{g}_0000.vtu");
        assert_eq!(
            std::fs::read(dir.join(&f)).unwrap(),
            std::fs::read(dir.join("ref").join(&f)).unwrap(),
            "{f}"
        );
    }
    assert_ne!(
        std::fs::read(dir.join("a/a-VTU/a_0000.vtu")).unwrap(),
        std::fs::read(dir.join("b/b-VTU/b_0000.vtu")).unwrap()
    );
    // the mesh dumper's field now names "b" (the old "a")
    let f = d.mesh(&m).dumper_names().collect::<Vec<_>>();
    assert_eq!(f, vec!["all"]);
    let dbg = format!("{:?}", d.mesh(&m).dumper("all").unwrap());
    assert!(dbg.contains("group: Some(\"b\")"), "{dbg}");
    assert!(!dbg.contains("group: Some(\"a\")"), "{dbg}");
}

/// ADR condition 1: one registry per mesh domain, the same for
/// every handle (every model on the mesh); never waits for its lock.
#[test]
fn one_registry_per_mesh_domain() {
    let dir = scratch("group_dumpers_shared");
    let mut m = two_triangles();
    tri_group(&mut m, "g", 0);
    let shared = SharedMesh::new(m);
    let model_a = SharedMeshDumpers::of(&shared).unwrap();
    let model_b = SharedMeshDumpers::of(&shared.clone()).unwrap();
    assert!(model_a.ptr_eq(&model_b));
    let other = SharedMeshDumpers::of(&SharedMesh::new(two_triangles())).unwrap();
    assert!(!model_a.ptr_eq(&other));
    let mesh = shared.read().unwrap();
    {
        let mut a = model_a.lock().unwrap();
        // re-entrant access is refused, never waited for
        assert!(model_b.lock().is_err());
        let g = a.element_group(&mesh, "g").unwrap();
        g.set_directory(&dir).unwrap();
        g.dump(&mesh, &NoFields, None, None).unwrap();
    }
    // the second model continues the same series
    let mut b = model_b.lock().unwrap();
    b.element_group(&mesh, "g")
        .unwrap()
        .dump(&mesh, &NoFields, None, None)
        .unwrap();
    assert_eq!(
        vtu_files(&dir.join("g-VTU")),
        vec!["g_0000.vtu", "g_0001.vtu"]
    );
}
