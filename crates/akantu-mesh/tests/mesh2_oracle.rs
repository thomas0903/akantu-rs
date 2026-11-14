// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests of
// Against the oracle fixtures fixtures/mesh2/* written by
// oracle/probes/mesh2 (upstream Mesh container queries and Mesh::read /
// MeshAccessor::makeReady state, ElementTypeMapArray::initialize(mesh, ...),
// ElementTypeMapArrayFilter, Mesh::getNbDataPerElem, the ElementGroup /
// NodeGroup default dumpers and group field factories, BoostGraphConverter).
// The Scotch cases (fixtures/mesh2/scotch/*) are in mesh2_scotch_oracle.rs
// (feature `scotch`).
//
// Every case rebuilds the oracle's input mesh (the same mesh file, checked by
// SHA-256, read with the akantu-io readers in upstream-parity mode, or the
// same MeshAccessor construction), runs the same operations and compares
// every recorded quantity exactly; a quantity no check consumed fails.

mod mesh2_common;

use akantu_core::{
    Element, ElementKind, ElementType, ElementTypeMapArray, GhostType, Idx, TypeFilter,
};
use akantu_io::dumper::group::{
    MeshDumpers, create_elemental_field, create_field_from_attached_data, create_nodal_field,
    create_strided_nodal_field,
};
use akantu_io::{
    DumpBuffer, FieldDescriptor, FieldResolver, FieldSource, FieldSupport, FieldValueType,
    MeshSelection,
};
use akantu_mesh::Mesh;
use akantu_mesh::graphviz::ElementGraph;
use akantu_mesh::type_map_filter::ElementTypeMapArrayFilter;
use akantu_mesh::type_map_init::{
    InitDimension, MeshInitialize, NbComponent, initialize_from_mesh,
};
use akantu_mesh::type_map_init::{nb_data_per_elem, nb_data_per_elem_shared};
use akantu_mesh::utils::init_mesh_facets;
use mesh2_common::{Case, block_name, cases, dim_name, ghost_name, kind_name, load_mesh, scratch};

const NG: GhostType = GhostType::NotGhost;
const DIMS: [Option<usize>; 5] = [None, Some(0), Some(1), Some(2), Some(3)];
const KINDS: [Option<ElementKind>; 4] = [
    None,
    Some(ElementKind::Regular),
    Some(ElementKind::Cohesive),
    Some(ElementKind::Structural),
];

/// Committed `fixtures/mesh2` cases per area (oracle/probes/mesh2/cases.txt;
/// the original cases plus adversarial cases: queries 11 + 2, groups_dump 2 + 3 + 3 kinds, scotch 6 + 13,
/// graphviz 5 + 1).
const EXPECTED: [(&str, usize); 4] = [
    ("queries", 13),
    ("groups_dump", 8),
    ("scotch", 19),
    ("graphviz", 6),
];

#[test]
fn every_mesh2_case_is_committed() {
    for (area, n) in EXPECTED {
        assert_eq!(cases(area).len(), n, "fixtures/mesh2/{area}");
    }
}

/// MESH-001…004: container state after `Mesh::read` / `makeReady`, element
/// counts and type iteration per `(dimension, ghost type, kind)`, facet
/// connectivities, groups.
fn check_queries(c: &mut Case, mesh: &Mesh) {
    c.eq_param("mesh/id", mesh.id());
    c.eq_param_u("mesh/spatial_dimension", mesh.spatial_dimension());
    c.eq_param_u("mesh/nb_nodes", mesh.nb_nodes());
    c.eq_param_u("mesh/nb_global_nodes", mesh.nb_global_nodes());
    c.eq_param_b("mesh/is_distributed", mesh.is_distributed());
    c.eq_param_b("mesh/is_periodic", mesh.is_periodic());
    c.eq_param_b("mesh/is_mesh_facets", mesh.is_mesh_facets());
    c.eq_param("mesh/nodes_id", mesh.nodes().id());
    c.eq_param("mesh/connectivities_id", mesh.connectivities().id());
    c.f64("bbox/lower", mesh.lower_bounds());
    c.f64("bbox/upper", mesh.upper_bounds());
    c.f64("bbox/local_lower", &mesh.local_bbox().lower);
    c.f64("bbox/local_upper", &mesh.local_bbox().upper);
    c.nodes("mesh", mesh);
    c.connectivities("mesh", mesh, true);
    for g in GhostType::ALL {
        for d in DIMS {
            for k in KINDS {
                let f = TypeFilter {
                    spatial_dimension: d,
                    ghost_type: g,
                    kind: k,
                };
                let key = format!("{}/{}/{}", dim_name(d), ghost_name(g), kind_name(k));
                let codes: Vec<i64> = mesh.element_types(f).map(|t| c.code(t)).collect();
                c.i64(&format!("element_types/{key}"), &codes);
                let n = mesh.nb_element_filtered(f);
                let name = format!("nb_element/{key}");
                match c.param(&name).clone() {
                    serde_json::Value::String(s) if d.is_none() => {
                        // UD-669: upstream's debug assertion rejects its own
                        // _all_dimensions; the port counts every type
                        assert!(s.contains("is greater than 3"), "{name}: {s}");
                        let sum: usize = mesh.element_types(f).map(|t| mesh.nb_element(t, g)).sum();
                        assert_eq!(n, sum, "{name}");
                        c.mark_param(&name);
                    }
                    _ => c.eq_param_u(&name, n),
                }
            }
        }
    }
    let keys = c.reference_i64("nb_element_type/keys");
    let mut values = Vec::new();
    for kg in keys.chunks(2) {
        let ty = c.type_of(kg[0]);
        let g = if kg[1] == 0 { NG } else { GhostType::Ghost };
        values.push(i64::try_from(mesh.nb_element(ty, g)).unwrap());
    }
    c.mark("nb_element_type/keys");
    c.i64("nb_element_type/values", &values);
    for g in GhostType::ALL {
        let types: Vec<ElementType> = mesh.element_types(TypeFilter::new().ghost(g)).collect();
        for ty in types {
            if ty.kind() == ElementKind::Structural {
                continue;
            }
            for t in 0..ty.nb_facet_types() {
                let mut v = Vec::new();
                for e in mesh
                    .elements(TypeFilter::new().ghost(g))
                    .filter(|e| e.ty() == ty)
                {
                    for facet in mesh.facet_connectivity(e, t).unwrap() {
                        v.extend(facet.iter().map(|&n| i64::from(n)));
                    }
                }
                c.i64(&format!("facet_connectivity/{}/{t}", block_name(ty, g)), &v);
            }
        }
    }
    c.groups("mesh", mesh);
}

/// MESH-017…019: `ElementTypeMapArray::initialize(mesh, ...)` variants.
fn check_typemap(c: &mut Case, mesh: &Mesh) {
    fn record<T: Copy>(c: &mut Case, name: &str, a: &ElementTypeMapArray<T>, to_f64: fn(T) -> f64) {
        let mut blocks = Vec::new();
        let mut ids = Vec::new();
        let mut values = Vec::new();
        for g in GhostType::ALL {
            for (ty, arr) in a.as_map().iter(g) {
                blocks.extend([
                    c.code(ty),
                    i64::from(g == GhostType::Ghost),
                    i64::try_from(arr.len()).unwrap(),
                    i64::try_from(arr.nb_component()).unwrap(),
                ]);
                ids.push(arr.id().to_owned());
                values.extend(arr.as_slice().iter().map(|&x| to_f64(x)));
            }
        }
        c.i64(&format!("typemap/{name}/blocks"), &blocks);
        c.f64(&format!("typemap/{name}/values"), &values);
        c.eq_param_strs(&format!("typemap/{name}/ids"), &ids);
        c.eq_param(&format!("typemap/{name}/id"), a.id());
    }
    let real = |x: f64| x;
    #[allow(clippy::cast_precision_loss)] // small exact integers
    let int = |x: i32| f64::from(x);
    let mut a = ElementTypeMapArray::<f64>::new("probe:default");
    initialize_from_mesh(&mut a, mesh, &MeshInitialize::default()).unwrap();
    record(c, "default", &a, real);
    let mut b = ElementTypeMapArray::<f64>::new("probe:nbelem");
    initialize_from_mesh(
        &mut b,
        mesh,
        &MeshInitialize {
            nb_component: NbComponent::Fixed(3),
            with_nb_element: true,
            default_value: 2.5,
            ..MeshInitialize::default()
        },
    )
    .unwrap();
    record(c, "nbelem", &b, real);
    initialize_from_mesh(
        &mut b,
        mesh,
        &MeshInitialize {
            nb_component: NbComponent::Fixed(3),
            ..MeshInitialize::default()
        },
    )
    .unwrap();
    record(c, "nbelem_reinit", &b, real);
    let mut d = ElementTypeMapArray::<i32>::new("probe:alldims_npe");
    initialize_from_mesh(
        &mut d,
        mesh,
        &MeshInitialize {
            nb_component: NbComponent::Fixed(2),
            spatial_dimension: InitDimension::All,
            with_nb_element: true,
            with_nb_nodes_per_element: true,
            default_value: -7,
            ..MeshInitialize::default()
        },
    )
    .unwrap();
    record(c, "alldims_npe", &d, int);
    let mut e = ElementTypeMapArray::<Idx>::new("probe:facets_regular");
    initialize_from_mesh(
        &mut e,
        mesh,
        &MeshInitialize {
            spatial_dimension: mesh
                .spatial_dimension()
                .checked_sub(1)
                .map_or(InitDimension::All, InitDimension::Dim),
            ghost_type: Some(NG),
            element_kind: Some(ElementKind::Regular),
            with_nb_element: true,
            default_value: 4,
            ..MeshInitialize::default()
        },
    )
    .unwrap();
    record(c, "facets_regular", &e, int);
    let per_type = |t: ElementType, g: GhostType| {
        t.nb_nodes_per_element() + usize::from(g == GhostType::Ghost)
    };
    let mut f = ElementTypeMapArray::<f64>::new("probe:functor");
    initialize_from_mesh(
        &mut f,
        mesh,
        &MeshInitialize {
            nb_component: NbComponent::PerType(&per_type),
            spatial_dimension: InitDimension::All,
            with_nb_element: true,
            ..MeshInitialize::default()
        },
    )
    .unwrap();
    record(c, "functor", &f, real);
    let mut filter = ElementTypeMapArray::<Idx>::new("probe:filter");
    let dim = mesh.spatial_dimension();
    let types: Vec<_> = mesh.element_types(TypeFilter::new().dim(dim)).collect();
    for t in types {
        let n = mesh.nb_element(t, NG);
        let ids: Vec<Idx> = (0..n)
            .step_by(2)
            .map(|e| Idx::try_from(e).unwrap())
            .collect();
        filter
            .alloc(ids.len(), 1, t, NG, 0)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&ids);
    }
    let mut h = ElementTypeMapArray::<f64>::new("probe:filtered");
    initialize_from_mesh(
        &mut h,
        mesh,
        &MeshInitialize {
            element_filter: Some(&filter),
            with_nb_element: true,
            nb_component: NbComponent::Fixed(2),
            default_value: 1.5,
            ..MeshInitialize::default()
        },
    )
    .unwrap();
    record(c, "filtered", &h, real);
}

/// MESH-020 (+ `getNbDataPerElem`): filtered views over every element
/// group, with and without `nb_data_per_elem`.
fn check_filter(c: &mut Case, mesh: &Mesh) {
    const NC: usize = 2;
    const NI: usize = 3;
    let mut data = ElementTypeMapArray::<f64>::new("probe:data");
    let mut nbd = akantu_core::ElementTypeMap::<usize>::new();
    let types: Vec<_> = mesh.element_types(TypeFilter::new()).collect();
    for t in types {
        let nb = mesh.nb_element(t, NG);
        let code = c.code(t);
        let a = data.alloc(nb * NI, NC, t, NG, 0.0).unwrap();
        for e in 0..nb {
            for q in 0..NI {
                for k in 0..NC {
                    #[allow(clippy::cast_precision_loss)] // small exact integers
                    let v = (1000 * e + 10 * q + k) as f64 + 0.25 * code as f64;
                    a.as_mut_slice()[(e * NI + q) * NC + k] = v;
                }
            }
        }
        nbd.insert(t, NG, NC * NI).unwrap();
    }
    let pairs = |c: &Case, m: &akantu_core::ElementTypeMap<usize>| -> Vec<i64> {
        let mut v = Vec::new();
        for (t, n) in m.iter(NG) {
            v.extend([c.code(t), i64::try_from(*n).unwrap()]);
        }
        v
    };
    let n = nb_data_per_elem(mesh, &data).unwrap();
    let v = pairs(c, &n);
    c.i64("nb_data_per_elem/data", &v);
    if let Ok(tags) = mesh.data().ints("tag_0") {
        let n = nb_data_per_elem_shared(mesh, tags).unwrap();
        let v = pairs(c, &n);
        c.i64("nb_data_per_elem/tag_0", &v);
    }
    let mut names = Vec::new();
    for g in mesh.groups().element_groups() {
        names.push(g.name().to_owned());
        for with_nbd in [false, true] {
            let f = if with_nbd {
                ElementTypeMapArrayFilter::with_nb_data_per_elem(&data, &g.elements, nbd.clone())
            } else {
                ElementTypeMapArrayFilter::new(&data, &g.elements)
            };
            let mut meta = Vec::new();
            let mut values = Vec::new();
            for t in f.element_types(TypeFilter::new()) {
                let af = f.get(t, NG).unwrap();
                meta.extend([
                    c.code(t),
                    i64::try_from(af.len()).unwrap(),
                    i64::try_from(af.nb_component()).unwrap(),
                ]);
                for tuple in af.iter_reinterpret(NC, af.len()).unwrap() {
                    values.extend_from_slice(tuple);
                }
            }
            let base = format!(
                "filter/{}/{}",
                g.name(),
                if with_nbd { "nbd" } else { "plain" }
            );
            c.i64(&format!("{base}/meta"), &meta);
            c.f64(&format!("{base}/values"), &values);
        }
    }
    c.eq_param_strs("filter/groups", &names);
}

#[test]
fn mesh_queries_type_maps_and_filters_match_the_oracle() {
    let mut failures = Vec::new();
    for id in cases("queries") {
        let mut c = Case::open("mesh2_queries", &id);
        let last = id.rsplit('/').next().unwrap();
        let ops = c.ops();
        if ops.contains("queries") {
            let mesh = load_mesh(&c, &format!("mesh2_{last}"));
            check_queries(&mut c, &mesh);
        }
        if ops.contains("typemap") {
            let mesh = load_mesh(&c, "typemap");
            check_typemap(&mut c, &mesh);
        }
        if ops.contains("filter") {
            let mesh = load_mesh(&c, "filter");
            check_filter(&mut c, &mesh);
        }
        if let Err(e) = c.finish() {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// MUTIL-017: `BoostGraphConverter(mesh).toGraphviz(file)` byte for byte.
#[test]
fn element_graphs_match_the_oracle_graphviz_files() {
    let mut failures = Vec::new();
    for id in cases("graphviz") {
        let mut c = Case::open("mesh2_graphviz", &id);
        let mut mesh = load_mesh(&c, "graphviz");
        let _ = init_mesh_facets(&mut mesh).unwrap();
        let g = ElementGraph::new(&mesh).unwrap();
        let dir = scratch(&format!("graphviz_{}", id.replace('/', "_")));
        let path = dir.join("graph.dot");
        g.write_graphviz(&path).unwrap();
        c.file("files/graph.dot", &std::fs::read(&path).unwrap());
        if let Err(e) = c.finish() {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The probe's nodal field `disp(n, c) = x(n, c) · (c + 1) + 0.5`.
struct Disp {
    values: Vec<f64>,
    nb_component: usize,
}

impl Disp {
    fn new(mesh: &Mesh) -> Self {
        let dim = mesh.spatial_dimension();
        let mut values = Vec::with_capacity(mesh.nb_nodes() * dim);
        for x in mesh.nodes().tuples() {
            for (k, &v) in x.iter().enumerate() {
                #[allow(clippy::cast_precision_loss)] // k ≤ 2
                values.push(v * (k + 1) as f64 + 0.5);
            }
        }
        Self {
            values,
            nb_component: dim,
        }
    }
}

impl FieldResolver for Disp {
    fn resolve(
        &self,
        _: &Mesh,
        f: &FieldDescriptor,
        ty: Option<ElementType>,
        _: GhostType,
    ) -> akantu_core::Result<DumpBuffer> {
        match (&f.source, ty) {
            (FieldSource::ModelNodal(n), None) if n == "disp" => Ok(DumpBuffer::Real {
                nb_component: self.nb_component,
                values: self.values.clone(),
            }),
            _ => Err(akantu_core::Error::not_found("probe field", f.name.clone())),
        }
    }
}

/// MESH-022, MESH-023, MESH-027: the default dumpers of every element group
/// (`paraview_<name>`, filtered mesh over all dimensions), of node groups
/// (`positions`), the factories (`createFieldFromAttachedData`,
/// `createNodalField`, `createStridedNodalField`, `createElementalField`)
/// and `getGroupDumper`, as `op_groups_dump` of the probe.
#[test]
fn group_dumpers_match_the_oracle_files() {
    let mut failures = Vec::new();
    for id in cases("groups_dump") {
        let mut c = Case::open("mesh2_groups_dump", &id);
        if !c.ops().contains("groups_dump") {
            continue; // kinds_group_dump cases: below
        }
        let mut mesh = load_mesh(&c, "groups");
        let dim = mesh.spatial_dimension();
        let out = scratch(&format!("groups_dump_{}", id.replace('/', "_")));
        let mut d = MeshDumpers::new();
        let disp = Disp::new(&mesh);
        let names: Vec<String> = mesh
            .groups()
            .element_groups()
            .map(|g| g.name().to_owned())
            .collect();
        c.eq_param_strs("group_dumpers/element_groups", &names);
        for n in &names {
            let dd = d.element_group(&mesh, n).unwrap();
            c.eq_param(
                &format!("group_dumpers/{n}/default"),
                dd.default_dumper_name().unwrap(),
            );
            dd.set_directory(&out.join("plain")).unwrap();
            dd.dump(&mesh, &disp, None, None).unwrap();
        }
        // a mixed-dimension group
        mesh.groups_mut()
            .create_element_group("mixed", dim)
            .unwrap();
        let all: Vec<_> = mesh.elements(TypeFilter::new()).collect();
        let mut picked = Vec::new();
        for ty in mesh.element_types(TypeFilter::new()) {
            picked.extend(all.iter().filter(|e| e.ty() == ty).step_by(3).copied());
        }
        for e in picked {
            mesh.element_group_add("mixed", e, true, true).unwrap();
        }
        mesh.groups_mut()
            .element_group_mut("mixed")
            .unwrap()
            .optimize();
        let mixed = d.element_group(&mesh, "mixed").unwrap();
        mixed.set_directory(&out.join("mixed")).unwrap();
        mixed.dump(&mesh, &disp, None, None).unwrap();
        // fields through the factories
        let mut vol = String::new();
        let mut facet = String::new();
        for g in mesh.groups().element_groups() {
            if g.name() == "mixed" {
                continue;
            }
            if g.dimension == dim && vol.is_empty() {
                vol = g.name().to_owned();
            }
            if Some(g.dimension) == dim.checked_sub(1) && facet.is_empty() {
                facet = g.name().to_owned();
            }
        }
        c.eq_param("group_dumpers/vol", &vol);
        c.eq_param("group_dumpers/facet", &facet);
        let real = FieldValueType::Real;
        let model = || FieldSource::ModelNodal("disp".into());
        if !vol.is_empty() {
            let fields = [
                create_field_from_attached_data(
                    &mesh,
                    "tag_0",
                    FieldValueType::Int,
                    &vol,
                    ElementKind::Regular,
                )
                .unwrap()
                .unwrap(),
                create_nodal_field(&mesh, "disp", model(), real, &vol, 3).unwrap(),
                create_strided_nodal_field(&mesh, "disp_1", model(), real, &vol, 1, 1, 0).unwrap(),
            ];
            let g = d.element_group(&mesh, &vol).unwrap();
            g.set_directory(&out.join("fields")).unwrap();
            for f in fields {
                g.add_dump_field(f).unwrap();
            }
            g.dump(&mesh, &disp, None, None).unwrap();
        }
        if !facet.is_empty() {
            let fields = [
                create_elemental_field(
                    &mesh,
                    "tag_0",
                    FieldSource::Mesh("tag_0".into()),
                    FieldSupport::Elemental,
                    FieldValueType::Int,
                    &facet,
                    Some(dim - 1),
                    ElementKind::Regular,
                    false,
                )
                .unwrap(),
                create_nodal_field(&mesh, "disp", model(), real, &facet, 0).unwrap(),
            ];
            let g = d.element_group(&mesh, &facet).unwrap();
            g.set_directory(&out.join("fields")).unwrap();
            for f in fields {
                g.add_dump_field(f).unwrap();
            }
            d.group_dumper(&mesh, &format!("paraview_{facet}"), &facet)
                .unwrap()
                .set_base_name(&format!("{facet}_via_mesh"))
                .unwrap();
            d.element_group(&mesh, &facet)
                .unwrap()
                .dump(&mesh, &disp, None, None)
                .unwrap();
        }
        // the mesh's own dumper ("all")
        {
            let fields = [
                create_field_from_attached_data(
                    &mesh,
                    "tag_0",
                    FieldValueType::Int,
                    "all",
                    ElementKind::Regular,
                )
                .unwrap()
                .unwrap(),
                create_nodal_field(&mesh, "disp", model(), real, "all", 3).unwrap(),
            ];
            let m = d.mesh(&mesh);
            m.register_paraview("mesh_all", Some("mesh_all"), true);
            m.set_directory_to_dumper("mesh_all", &out.join("all"))
                .unwrap();
            m.add_dump_mesh_to_dumper(
                "mesh_all",
                MeshSelection::new(Some(dim), NG, ElementKind::Regular),
            )
            .unwrap();
            for f in fields {
                m.add_dump_field_to_dumper("mesh_all", f).unwrap();
            }
            d.group_dumper(&mesh, "mesh_all", "all")
                .unwrap()
                .set_base_name("mesh_all_via_group_dumper")
                .unwrap();
            d.mesh(&mesh)
                .dump_named("mesh_all", &mesh, &disp, None, None)
                .unwrap();
        }
        // node groups
        {
            let ng = mesh.groups_mut().create_node_group("corner_nodes").unwrap();
            for n in (0..ng_len(&disp)).step_by(4) {
                ng.add(Idx::try_from(n).unwrap(), true);
            }
        }
        let corner = d.node_group(&mesh, "corner_nodes").unwrap();
        c.eq_param(
            "group_dumpers/corner_nodes/default",
            corner.default_dumper_name().unwrap(),
        );
        corner.set_directory(&out.join("nodes")).unwrap();
        corner.dump(&mesh, &disp, None, None).unwrap();
        if !vol.is_empty() {
            let v = d.node_group(&mesh, &format!("{vol}_nodes")).unwrap();
            v.set_directory(&out.join("nodes")).unwrap();
            v.dump(&mesh, &disp, None, None).unwrap();
        }
        c.tree("groups_dump/", &out);
        if let Err(e) = c.finish() {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Adversarial mode `kinds_group_dump` (it fails if the default element
/// kind of a group dumper is `_ek_regular` instead of every kind): the default dumper of an
/// element group holding cohesive, structural, segment and point elements
/// (every element of the generated mesh, added in reverse order; dumped
/// before and after `optimize`) and of a node group filled in reverse
/// order, byte for byte. Upstream's default element-group selection is
/// every kind (`_ek_not_defined`).
#[test]
fn kinds_group_dumps_match_the_oracle_files() {
    let mut failures = Vec::new();
    let mut seen = 0;
    for id in cases("groups_dump") {
        let mut c = Case::open("mesh2_kinds_group_dump", &id);
        if !c.ops().contains("kinds_group_dump") {
            continue;
        }
        seen += 1;
        let file = c.spec("file").unwrap();
        let generated = file.strip_prefix("gen:").unwrap().to_owned();
        let mut mesh = load_mesh(&c, &format!("kinds_{generated}"));
        let out = scratch(&format!("kinds_{}", id.replace('/', "_")));
        let disp = Disp::new(&mesh);
        let gname = format!("all_{generated}");
        let mut elements: Vec<(Element, Vec<Idx>)> = Vec::new();
        for ty in mesh.element_types(TypeFilter::new().ghost(NG)) {
            let conn = mesh.connectivity(ty, NG).unwrap();
            for e in (0..conn.len()).rev() {
                elements.push((
                    Element::new(ty, Idx::try_from(e).unwrap(), NG),
                    conn.tuple(e).to_vec(),
                ));
            }
        }
        {
            let dim = mesh.spatial_dimension();
            let mut g = mesh.groups_mut().create_element_group(&gname, dim).unwrap();
            for (el, conn) in &elements {
                g.add(*el, Some(conn), true).unwrap();
            }
        }
        let mut d = MeshDumpers::new();
        let g = d.element_group(&mesh, &gname).unwrap();
        g.set_directory(&out).unwrap();
        g.dump(&mesh, &disp, None, None).unwrap();
        mesh.groups_mut()
            .element_group_mut(&gname)
            .unwrap()
            .optimize();
        d.element_group(&mesh, &gname)
            .unwrap()
            .dump(&mesh, &disp, None, None)
            .unwrap();
        let nname = format!("odd_{generated}");
        {
            let n = mesh.nb_nodes();
            let ng = mesh.groups_mut().create_node_group(&nname).unwrap();
            for node in (0..n).rev().step_by(2) {
                ng.add(Idx::try_from(node).unwrap(), true);
            }
        }
        let ngd = d.node_group(&mesh, &nname).unwrap();
        ngd.set_directory(&out).unwrap();
        ngd.dump(&mesh, &disp, None, None).unwrap();
        c.eq_param("kinds/element_group", &gname);
        c.eq_param_u(
            "kinds/element_group_nodes",
            mesh.groups().element_group(&gname).unwrap().nb_nodes(),
        );
        c.eq_param("kinds/node_group", &nname);
        c.eq_param_u(
            "kinds/node_group_size",
            mesh.groups().node_group(&nname).unwrap().len(),
        );
        c.tree("kinds/", &out);
        if let Err(e) = c.finish() {
            failures.push(e);
        }
    }
    assert_eq!(seen, 3, "kinds_group_dump cases");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn ng_len(d: &Disp) -> usize {
    d.values.len() / d.nb_component.max(1)
}
