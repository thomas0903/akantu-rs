// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests of
// the mesh utilities against the oracle fixtures
// `fixtures/mesh_utils/*` written by oracle/probes/mesh_utils (upstream
// Mesh::initMeshFacets, MeshUtils::buildFacets / buildNode2Elements /
// purifyMesh / fillElementToSubElementsData, Mesh::eraseElements,
// GroupManager::createClusters / createBoundaryGroupFromGeometry,
// Mesh::makePeriodic / makeConsistentForPeriodicity).
//
// Every case rebuilds the oracle's input mesh from the fixture (coordinates
// bit for bit, connectivities, physical names → groups, as Mesh::read leaves
// them), runs the same operations and compares every recorded quantity
// exactly; a quantity of the fixture that no check consumed is a failure.

use std::collections::BTreeSet;

use akantu_core::enums::MeshIoType;
use akantu_core::enums::{ReduceOperation, SpatialDirection};
use akantu_core::{Array, Element, ElementType, GhostType, Idx, Real, TypeFilter};
use akantu_io::{ReadOptions, read_mesh_with};
use akantu_mesh::clusters::{ClusterFacets, create_boundary_group_from_geometry, create_clusters};
use akantu_mesh::periodic::{make_consistent_for_periodicity_real, make_periodic};
use akantu_mesh::removal::erase_elements_setup;
use akantu_mesh::utils::{
    ELEMENT_TO_SUBELEMENT, MESH_TO_MESH_FACET, NodeToElements, PHYSICAL_NAMES,
    SUBELEMENT_TO_ELEMENT, build_facets, build_node_to_elements, build_node_to_elements_of_type,
    fill_nodes_to_elements, init_mesh_facets, purify_mesh,
};
use akantu_mesh::{Mesh, MeshAccessor, MeshDataValues};
use akantu_validation::{DiffTest, Fixture, fixtures_root};
use serde_json::Value;

/// Committed `fixtures/mesh_utils` cases (oracle/probes/mesh_utils/cases.txt).
const EXPECTED_CASES: usize = 55;

fn cases() -> Vec<String> {
    let root = fixtures_root().join("mesh_utils");
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if p.join("manifest.json").exists() {
                    let rel = p.strip_prefix(fixtures_root()).unwrap();
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                } else {
                    stack.push(p);
                }
            }
        }
    }
    out.sort();
    out
}

/// Block name `"<type>"` or `"<type>/ghost"`.
fn parse_block(b: &str) -> (ElementType, GhostType) {
    match b.strip_suffix("/ghost") {
        Some(t) => (t.parse().unwrap(), GhostType::Ghost),
        None => (b.parse().unwrap(), GhostType::NotGhost),
    }
}

fn block_name(ty: ElementType, g: GhostType) -> String {
    if g == GhostType::Ghost {
        format!("{ty}/ghost")
    } else {
        ty.to_string()
    }
}

fn str_list(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

/// One differential case: the fixture, the harness and the set of checked
/// quantities.
struct Case {
    t: DiffTest,
    params: Value,
    type_table: Vec<String>,
    checked: BTreeSet<String>,
    problems: Vec<String>,
}

impl Case {
    fn encode(&self, e: Element, out: &mut Vec<i64>) {
        if e.is_null() {
            out.extend([-1, -1, -1]);
            return;
        }
        let code = self
            .type_table
            .iter()
            .position(|n| n == e.ty().name())
            .map_or(-99, |p| i64::try_from(p).unwrap());
        out.extend([code, i64::from(e.index()), i64::from(e.ghost_type() as u8)]);
    }

    /// Expected element triples with upstream's non-canonical default
    /// `Element()` (type code -2) mapped to the canonical NULL.
    fn i64(&mut self, name: &str, actual: &[i64]) {
        self.checked.insert(name.to_owned());
        if !self.t.check_i64(name, actual) {
            self.problems.push(name.to_owned());
        }
    }

    fn f64(&mut self, name: &str, actual: &[f64]) {
        self.checked.insert(name.to_owned());
        if !self.t.check_f64(name, actual) {
            self.problems.push(name.to_owned());
        }
    }

    fn param(&self, key: &str) -> &Value {
        &self.params[key]
    }

    fn eq_param<T: PartialEq + std::fmt::Debug>(&mut self, what: &str, expected: T, actual: T) {
        if expected != actual {
            self.problems
                .push(format!("{what}: expected {expected:?}, got {actual:?}"));
        }
    }

    fn elements(&self, list: &[Element]) -> Vec<i64> {
        let mut v = Vec::new();
        for &e in list {
            self.encode(e, &mut v);
        }
        v
    }

    fn connectivities(&mut self, prefix: &str, mesh: &Mesh) {
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            for ty in mesh.element_types(TypeFilter::new().ghost(g)) {
                blocks.push(block_name(ty, g));
                let c = mesh.connectivity(ty, g).unwrap();
                let v: Vec<i64> = c.as_slice().iter().map(|&n| i64::from(n)).collect();
                self.i64(&format!("{prefix}/connectivity/{}", block_name(ty, g)), &v);
            }
        }
        let expected = str_list(self.param(&format!("{prefix}/blocks")));
        self.eq_param(&format!("{prefix}/blocks"), expected, blocks);
    }

    fn element_lists(&mut self, prefix: &str, mesh: &Mesh, name: &str) {
        let key = format!("{prefix}/{name}/blocks");
        let expected = str_list(self.param(&key));
        let Ok(MeshDataValues::ElementList(m)) = mesh.data().elemental(name) else {
            self.eq_param(&key, expected, Vec::new());
            return;
        };
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            for ty in m.element_types(TypeFilter::new().ghost(g)) {
                let b = block_name(ty, g);
                let a = m.get(ty, g).unwrap();
                let mut offsets = vec![0i64];
                let mut values = Vec::new();
                for row in a.as_slice() {
                    for &e in row {
                        self.encode(e, &mut values);
                    }
                    offsets.push(i64::try_from(values.len() / 3).unwrap());
                }
                self.i64(&format!("{prefix}/{name}/{b}/offsets"), &offsets);
                self.i64(&format!("{prefix}/{name}/{b}/values"), &values);
                blocks.push(b);
            }
        }
        self.eq_param(&key, expected, blocks);
    }

    fn element_refs(&mut self, prefix: &str, mesh: &Mesh, name: &str) {
        let key = format!("{prefix}/{name}/blocks");
        let expected = str_list(self.param(&key));
        let Ok(MeshDataValues::Element(m)) = mesh.data().elemental(name) else {
            self.eq_param(&key, expected, Vec::new());
            return;
        };
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            for ty in m.element_types(TypeFilter::new().ghost(g)) {
                let b = block_name(ty, g);
                let v = self.elements(m.get(ty, g).unwrap().as_slice());
                let q = format!("{prefix}/{name}/{b}");
                // upstream default Element() (code -2) is the canonical NULL here
                let mut expected_data =
                    self.t.reference_i64(&q).map(|f| f.data).unwrap_or_default();
                for chunk in expected_data.chunks_mut(3) {
                    if chunk[0] == -2 {
                        chunk.copy_from_slice(&[-1, -1, -1]);
                    }
                }
                self.checked.insert(q.clone());
                if expected_data != v {
                    self.problems
                        .push(format!("{q}: element references differ"));
                }
                blocks.push(b);
            }
        }
        self.eq_param(&key, expected, blocks);
    }

    fn csr(&mut self, prefix: &str, map: &NodeToElements) {
        let offsets: Vec<i64> = map
            .offsets
            .iter()
            .map(|&o| i64::try_from(o).unwrap())
            .collect();
        let values = self.elements(&map.elements);
        self.i64(&format!("{prefix}/offsets"), &offsets);
        self.i64(&format!("{prefix}/values"), &values);
    }

    fn groups(&mut self, prefix: &str, mesh: &Mesh) {
        let expected = self.param(&format!("{prefix}/element_groups")).clone();
        let expected = expected.as_array().cloned().unwrap_or_default();
        let actual: Vec<_> = mesh.groups().element_groups().collect();
        self.eq_param(
            &format!("{prefix}/element_groups (count)"),
            expected.len(),
            actual.len(),
        );
        for (e, g) in expected.iter().zip(actual) {
            let name = e["name"].as_str().unwrap();
            self.eq_param(&format!("{prefix} group name"), name, g.name());
            let dim = usize::try_from(e["dimension"].as_i64().unwrap()).unwrap();
            self.eq_param(
                &format!("{prefix} group {name} dimension"),
                dim,
                g.dimension,
            );
            self.eq_param(
                &format!("{prefix} group {name} node group"),
                e["node_group"].as_str().unwrap(),
                g.node_group.name(),
            );
            let mut safe = name.replace(|c: char| c.is_whitespace() || c == '/', "_");
            if safe.is_empty() {
                safe = "_".into();
            }
            let mut blocks = Vec::new();
            for gt in GhostType::ALL {
                for ty in g.element_types(gt) {
                    let b = block_name(ty, gt);
                    let v: Vec<i64> = g
                        .elements_of(ty, gt)
                        .iter()
                        .map(|&x| i64::from(x))
                        .collect();
                    self.i64(&format!("{prefix}/element_groups/{safe}/{b}"), &v);
                    blocks.push(b);
                }
            }
            self.eq_param(
                &format!("{prefix} group {name} blocks"),
                str_list(&e["blocks"]),
                blocks,
            );
            let nodes: Vec<i64> = g.nodes().iter().map(|&x| i64::from(x)).collect();
            self.i64(&format!("{prefix}/element_groups/{safe}/nodes"), &nodes);
        }
        let ng: Vec<String> = mesh
            .groups()
            .node_groups()
            .map(|n| n.name().to_owned())
            .collect();
        let expected_ng = str_list(self.param(&format!("{prefix}/node_groups")));
        self.eq_param(&format!("{prefix}/node_groups"), expected_ng, ng);
    }

    fn strings(&mut self, prefix: &str, mesh: &Mesh, name: &str) {
        let Ok(m) = mesh.data().strings(name) else {
            return;
        };
        let tables = self.param("string_tables").clone();
        for g in GhostType::ALL {
            for ty in m.element_types(TypeFilter::new().ghost(g)) {
                let q = format!("{prefix}/{name}/{}", block_name(ty, g));
                let table = str_list(&tables[&q]);
                let codes: Vec<i64> = m
                    .get(ty, g)
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|s| {
                        table
                            .iter()
                            .position(|t| t == s)
                            .map_or(-1, |p| i64::try_from(p).unwrap())
                    })
                    .collect();
                self.i64(&q, &codes);
            }
        }
    }
}

/// The oracle's input mesh, as `Mesh::read` leaves it: read meshes go
/// through the akantu-io readers (verified against the oracle's
/// `fixtures/mesh_io`, upstream parity options: UD-045 off), which end with
/// `fillElementToSubElementsData` as upstream's do (the
/// `read/element_to_subelement` and `read/subelement_to_element`
/// quantities compare the readers' own output with upstream's `Mesh::read`
/// exactly); generated meshes are rebuilt from the recorded arrays (the
/// probe builds them with `MeshAccessor`, no reader). Coordinates and
/// connectivities are checked against the oracle's (`input/*`).
fn build_input(fx: &Fixture, params: &Value) -> Mesh {
    let dim = usize::try_from(params["spatial_dimension"].as_i64().unwrap()).unwrap();
    let mut m = Mesh::new(dim, "mesh").unwrap();
    let read = params["source"]
        .as_str()
        .is_some_and(|s| s.starts_with("up:") || s.starts_with("fz:") || s.starts_with("in:"));
    if read {
        let opts = ReadOptions {
            fill_unnamed_physical_groups: false,
            write_diana_material_file: false,
        };
        read_mesh_with(
            &mut m,
            &fx.input_path("mesh").unwrap(),
            MeshIoType::Auto,
            &opts,
        )
        .unwrap();
        return m;
    }
    let nodes = fx.load_f64("input/nodes").unwrap();
    let blocks = str_list(&params["input/blocks"]);
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in nodes.data.chunks_exact(dim.max(1)) {
            a.push_node(x).unwrap();
        }
        for b in &blocks {
            let (ty, g) = parse_block(b);
            let c = fx.load_i64(&format!("input/connectivity/{b}")).unwrap();
            let mut conn = a.connectivity_mut(ty, g);
            for v in c.data.chunks_exact(ty.nb_nodes_per_element()) {
                let v: Vec<Idx> = v.iter().map(|&n| Idx::try_from(n).unwrap()).collect();
                conn.push_tuple(&v);
            }
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

fn check_input(c: &mut Case, fx: &Fixture, mesh: &Mesh) {
    c.f64("input/nodes", mesh.nodes().as_slice());
    let blocks = str_list(c.param("input/blocks"));
    let mut actual_blocks = Vec::new();
    for g in GhostType::ALL {
        for ty in mesh.element_types(TypeFilter::new().ghost(g)) {
            let b = block_name(ty, g);
            let v: Vec<i64> = mesh
                .connectivity(ty, g)
                .unwrap()
                .as_slice()
                .iter()
                .map(|&n| i64::from(n))
                .collect();
            c.i64(&format!("input/connectivity/{b}"), &v);
            actual_blocks.push(b);
        }
    }
    c.eq_param("input/blocks", blocks, actual_blocks);
    c.strings("input", mesh, PHYSICAL_NAMES);
    for q in fx.quantity_names("input/physical_names/*") {
        if !c.checked.contains(q) {
            c.problems.push(format!("missing physical names {q}"));
        }
    }
    let lower: Vec<Real> = c
        .param("bounds_lower")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    let upper: Vec<Real> = c
        .param("bounds_upper")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    c.eq_param("bounds_lower", lower, mesh.lower_bounds().to_vec());
    c.eq_param("bounds_upper", upper, mesh.upper_bounds().to_vec());
    c.element_lists("read", mesh, ELEMENT_TO_SUBELEMENT);
    c.element_refs("read", mesh, SUBELEMENT_TO_ELEMENT);
}

fn copy(mesh: &Mesh, fx: &Fixture, params: &Value) -> Mesh {
    let _ = mesh;
    build_input(fx, params)
}

fn run_case(case_id: &str) -> Result<(), String> {
    let fx = Fixture::open_by_id(case_id).map_err(|e| e.to_string())?;
    let params = fx.manifest().parameters.clone();
    let t = DiffTest::new(
        &format!("mesh_utils::{}", case_id.replace('/', "::")),
        case_id,
    )
    .map_err(|e| e.to_string())?;
    let mut c = Case {
        t,
        type_table: str_list(&params["type_table"]),
        params: params.clone(),
        checked: BTreeSet::new(),
        problems: Vec::new(),
    };
    let ops: BTreeSet<String> = params["ops"]
        .as_str()
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let base = build_input(&fx, &params);
    let dim = base.spatial_dimension();
    check_input(&mut c, &fx, &base);
    if ops.contains("iterators") {
        c.groups("read", &base);
    }
    if ops.contains("csr") {
        let csr = build_node_to_elements(&base, None).map_err(|e| e.to_string())?;
        c.csr("csr", &csr);
        let ty: ElementType = c.param("csr_type/type").as_str().unwrap().parse().unwrap();
        let typed = build_node_to_elements_of_type(&base, ty, GhostType::NotGhost).unwrap();
        let off: Vec<i64> = typed
            .offsets
            .iter()
            .map(|&o| i64::try_from(o).unwrap())
            .collect();
        let val: Vec<i64> = typed.elements.iter().map(|&e| i64::from(e)).collect();
        c.i64("csr_type/offsets", &off);
        c.i64("csr_type/values", &val);
        let sets = fill_nodes_to_elements(&base, None).unwrap();
        c.csr("nodes_to_elements", &sets);
    }
    if ops.contains("facets") {
        let mut m = copy(&base, &fx, &params);
        let _ = init_mesh_facets(&mut m).map_err(|e| e.to_string())?;
        let f = m.mesh_facets().unwrap();
        c.connectivities("facets", f);
        c.element_lists("facets", f, ELEMENT_TO_SUBELEMENT);
        c.element_refs("facets", f, SUBELEMENT_TO_ELEMENT);
        c.strings("facets", f, PHYSICAL_NAMES);
        c.groups("facets", f);
        let key = format!("parent/{MESH_TO_MESH_FACET}/blocks");
        if c.param(&key).is_array() {
            c.element_refs("parent", &m, MESH_TO_MESH_FACET);
        } else if m.data().has_elemental(MESH_TO_MESH_FACET) {
            c.problems.push("unexpected mesh_to_mesh_facet".into());
        }
    }
    if ops.contains("buildfacets") {
        let mut m = copy(&base, &fx, &params);
        let _ = build_facets(&mut m).map_err(|e| e.to_string())?;
        c.connectivities("build_facets", &m);
        c.element_lists("build_facets", &m, ELEMENT_TO_SUBELEMENT);
        c.element_refs("build_facets", &m, SUBELEMENT_TO_ELEMENT);
        if ops.contains("boundary") {
            let nb = create_boundary_group_from_geometry(&mut m).map_err(|e| e.to_string())?;
            let e = c.param("boundary/nb_clusters").as_i64().unwrap();
            c.eq_param("boundary/nb_clusters", e, i64::try_from(nb).unwrap());
            c.groups("boundary", &m);
        }
    } else if ops.contains("boundary") {
        let mut m = copy(&base, &fx, &params);
        let nb = create_boundary_group_from_geometry(&mut m).map_err(|e| e.to_string())?;
        let e = c.param("boundary/nb_clusters").as_i64().unwrap();
        c.eq_param("boundary/nb_clusters", e, i64::try_from(nb).unwrap());
        c.groups("boundary", &m);
    }
    if ops.contains("clusters") {
        let mut m = copy(&base, &fx, &params);
        let nb = create_clusters(
            &mut m,
            dim,
            "cluster",
            &|el| el.index() % 3 != 0,
            ClusterFacets::Build,
        )
        .map_err(|e| e.to_string())?;
        let e = c.param("clusters/nb_clusters").as_i64().unwrap();
        c.eq_param("clusters/nb_clusters", e, i64::try_from(nb).unwrap());
        c.groups("clusters", &m);
    }
    if ops.contains("purify") {
        let mut m = copy(&base, &fx, &params);
        let _ = purify_mesh(&mut m).map_err(|e| e.to_string())?;
        c.f64("purify/nodes", m.nodes().as_slice());
        c.connectivities("purify", &m);
    }
    if ops.contains("erase") {
        let mut m = copy(&base, &fx, &params);
        let list = fx.load_i64("erase/elements").unwrap();
        let tt = c.type_table.clone();
        let elements: Vec<Element> = list
            .data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|e| {
                let ty: ElementType = tt[usize::try_from(e[0]).unwrap()].parse().unwrap();
                let g = if e[2] == 0 {
                    GhostType::NotGhost
                } else {
                    GhostType::Ghost
                };
                Element::new(ty, Idx::try_from(e[1]).unwrap(), g)
            })
            .collect();
        let echo = c.elements(&elements);
        c.i64("erase/elements", &echo);
        let _ = erase_elements_setup(&mut m, &elements).map_err(|e| e.to_string())?;
        c.connectivities("erase", &m);
    }
    if ops.contains("periodic") || ops.contains("periodic_groups") {
        let mut m = copy(&base, &fx, &params);
        let dirs = c.param("periodic/directions").as_str().unwrap().to_owned();
        for ch in dirs.chars() {
            let d = match ch {
                'x' => SpatialDirection::X,
                'y' => SpatialDirection::Y,
                _ => SpatialDirection::Z,
            };
            let (a, b) = (format!("{ch}-minus"), format!("{ch}-plus"));
            let groups = ops
                .contains("periodic_groups")
                .then_some((a.as_str(), b.as_str()));
            let _ = make_periodic(&mut m, d, groups).map_err(|e| e.to_string())?;
        }
        let e = c.param("periodic/is_periodic").as_bool().unwrap();
        c.eq_param("periodic/is_periodic", e, m.is_periodic());
        let pairs = akantu_mesh::periodic::periodic_pairs(&m);
        let pv: Vec<i64> = pairs
            .iter()
            .flat_map(|&(a, b)| [i64::from(a), i64::from(b)])
            .collect();
        c.i64("periodic/pairs", &pv);
        let mut sm: Vec<(Idx, Idx)> = pairs
            .iter()
            .map(|&(ma, s)| (s, m.periodic_master(s).unwrap_or(ma)))
            .collect();
        sm.sort_unstable();
        let smv: Vec<i64> = sm
            .iter()
            .flat_map(|&(a, b)| [i64::from(a), i64::from(b)])
            .collect();
        c.i64("periodic/slave_master", &smv);
        let flags: Vec<i64> = m
            .nodes_flags()
            .as_slice()
            .iter()
            .map(|f| i64::from(f.bits()))
            .collect();
        c.i64("periodic/flags", &flags);
        // upstream test_mesh_periodic MakeConsistentForPeriodicitySum
        let n = m.nb_nodes();
        let mut shared = vec![0.0; n];
        for ch in dirs.chars() {
            for g in [format!("{ch}-minus"), format!("{ch}-plus")] {
                if let Ok(g) = m.groups().element_group(&g) {
                    for &node in g.nodes().as_slice() {
                        shared[usize::try_from(node).unwrap()] += 1.0;
                    }
                }
            }
        }
        let mut values = Array::filled(n, 1, 100.0);
        let masters: BTreeSet<Idx> = pairs.iter().map(|p| p.0).collect();
        for &ma in &masters {
            if m.is_local_or_master_node(ma).unwrap() {
                let k = shared[usize::try_from(ma).unwrap()];
                values.as_mut_slice()[usize::try_from(ma).unwrap()] = 100.0 / 2f64.powf(k);
            }
        }
        c.f64("periodic/sum_input", values.as_slice());
        make_consistent_for_periodicity_real(&m, &mut values, ReduceOperation::Id).unwrap();
        c.f64("periodic/after_id", values.as_slice());
        make_consistent_for_periodicity_real(&m, &mut values, ReduceOperation::Sum).unwrap();
        c.f64("periodic/after_sum", values.as_slice());
    }
    // every recorded quantity must have been compared
    let all: BTreeSet<String> = fx
        .quantity_names("*")
        .into_iter()
        .map(str::to_owned)
        .collect();
    for q in all.difference(&c.checked) {
        c.problems.push(format!("quantity {q} not compared"));
    }
    let problems = std::mem::take(&mut c.problems);
    let report = c.t.finish();
    match (report, problems.is_empty()) {
        (Ok(_), true) => Ok(()),
        (Ok(_), false) => Err(problems.join("\n")),
        (Err(f), _) => Err(format!("{f}\n{}", problems.join("\n"))),
    }
}

/// Every `mesh_utils` fixture: facets (numbering, node order, adjacency
/// content and slot order), CSR maps, in-place boundary facets, boundary
/// groups and clusters, purify and erase renumberings, periodic pairing and
/// consistency, equal to the oracle (exact integers, bitwise reals).
#[test]
fn every_mesh_utils_fixture_matches_the_oracle() {
    let cases = cases();
    assert_eq!(cases.len(), EXPECTED_CASES, "{cases:?}");
    let mut failures = Vec::new();
    for case in &cases {
        if let Err(e) = run_case(case) {
            failures.push(format!("{case}:\n{e}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

/// IO-011 (re-verification): the Diana
/// reader ends with `fillElementToSubElementsData` as upstream
/// (`mesh_io_diana.cc:174`); on the two mixed-dimension Diana meshes of
/// `oracle/probes/mesh_utils/inputs` its `element_to_subelement` /
/// `subelement_to_element` data equal upstream's `Mesh::read` exactly
/// (`mesh_utils/read/diana_{hexa_quad,two_hexa}`). The oracle data are
/// non-trivial: in `diana_two_hexa` the shared quadrangle
/// lists both hexahedra and each hexahedron lists its two quadrangles in
/// the first slots, the remaining slots NULL.
#[test]
fn io_011_diana_reader_adjacency_matches_the_oracle() {
    for case in [
        "mesh_utils/read/diana_hexa_quad",
        "mesh_utils/read/diana_two_hexa",
    ] {
        run_case(case).unwrap_or_else(|e| panic!("{case}: {e}"));
    }
    let fx = Fixture::open_by_id("mesh_utils/read/diana_two_hexa").unwrap();
    let table = str_list(&fx.manifest().parameters["type_table"]);
    assert_eq!(table, ["_hexahedron_8", "_quadrangle_4"]);
    let (hex, null) = (0, [-1, -1, -1]);
    let e2s_offsets = fx
        .load_i64("read/element_to_subelement/_quadrangle_4/offsets")
        .unwrap()
        .data;
    let e2s = fx
        .load_i64("read/element_to_subelement/_quadrangle_4/values")
        .unwrap()
        .data;
    assert_eq!(
        e2s_offsets,
        [0, 2, 3, 4],
        "quadrangles 0 (shared), 1 (top), 2 (bottom)"
    );
    assert_eq!(e2s, [hex, 0, 0, hex, 1, 0, hex, 1, 0, hex, 0, 0]);
    let s2e = fx
        .load_i64("read/subelement_to_element/_hexahedron_8")
        .unwrap()
        .data;
    let quad = 1;
    let mut want = Vec::new();
    for slots in [[0, 2], [0, 1]] {
        for q in slots {
            want.extend([quad, q, 0]);
        }
        for _ in 0..4 {
            want.extend(null);
        }
    }
    assert_eq!(s2e, want);
}
