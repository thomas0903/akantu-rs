// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential-test
// helpers comparing meshes read by `akantu_io` with `mesh_probe` fixtures
// (oracle/probes/cpp/mesh_probe.cc, which records what Akantu's
// `Mesh::read` produces).

#![allow(dead_code)] // each test binary uses a subset of the helpers

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, GhostType, TypeFilter};
use akantu_io::{ReadOptions, read_mesh_with};
use akantu_mesh::{Mesh, MeshDataValues, NodalDataValues};
use akantu_validation::manifest::StorageMode;
use akantu_validation::{DiffTest, Fixture, fixtures_root};

/// Every fixture case directory below `root`, sorted.
pub fn case_dirs(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                if p.join("manifest.json").is_file() {
                    out.push(p.clone());
                }
                walk(&p, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// `mesh_io` fixture cases.
pub fn mesh_io_cases() -> Vec<PathBuf> {
    case_dirs(&fixtures_root().join("mesh_io"))
}

/// The fixture cases below a root, split by whether their data are present.
/// Committed cases must always be materialised; *external* cases
/// (size policy: only the manifest is committed) are skipped with their
/// regeneration command when their data are absent, as in a fresh clone,
/// and compared when they have been regenerated.
#[derive(Debug, Default)]
pub struct CaseSet {
    /// Cases whose data files are all present.
    pub materialized: Vec<Fixture>,
    /// External cases without local data: `(case id, regeneration command)`.
    pub external_missing: Vec<(String, String)>,
    /// Committed cases with missing data files (always an error).
    pub broken: Vec<String>,
}

impl CaseSet {
    /// Partition `dirs`.
    pub fn new(dirs: &[PathBuf]) -> Self {
        let mut set = Self::default();
        for dir in dirs {
            let fx = Fixture::open(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            if fx.check_materialized().is_ok() {
                set.materialized.push(fx);
            } else if fx.manifest().storage.mode == StorageMode::External {
                let cmd = fx.manifest().storage.generate.clone().unwrap_or_default();
                set.external_missing.push((fx.case_id().to_owned(), cmd));
            } else {
                set.broken.push(fx.case_id().to_owned());
            }
        }
        set
    }

    /// Number of cases.
    pub fn len(&self) -> usize {
        self.materialized.len() + self.external_missing.len() + self.broken.len()
    }

    /// Number of committed (non-external) cases among the materialised ones.
    pub fn committed(&self) -> usize {
        self.materialized
            .iter()
            .filter(|f| f.manifest().storage.mode != StorageMode::External)
            .count()
    }

    /// Fail on committed cases without data; print the skipped external
    /// cases (count and regeneration commands).
    pub fn check_and_announce(&self, test: &str) {
        assert!(
            self.broken.is_empty(),
            "{test}: committed fixture(s) with missing data files (check the checkout): {:?}",
            self.broken
        );
        if !self.external_missing.is_empty() {
            eprintln!(
                "{test}: {} external fixture(s) not materialised, skipped (regenerate to compare them):",
                self.external_missing.len()
            );
            for (id, cmd) in &self.external_missing {
                eprintln!("  {id}: {cmd}");
            }
        }
    }
}

/// Path of the fixture's input mesh: the stored input, or, for an external
/// case whose input is not materialised, the frozen Gmsh mesh it was
/// generated from (`fixtures/meshes/gmsh-<version>/`, D-009) when that file
/// has the input's SHA-256. `None` if neither is available.
pub fn mesh_input(fx: &Fixture) -> Option<PathBuf> {
    if let Ok(p) = fx.input_path("mesh") {
        return Some(p);
    }
    let input = fx.manifest().inputs.iter().find(|i| i.role == "mesh")?;
    let rel = input
        .source
        .as_deref()?
        .strip_prefix("upstream:generated/")?;
    let frozen = std::fs::read_dir(fixtures_root().join("meshes")).ok()?;
    let mut roots: Vec<PathBuf> = frozen
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    roots.sort();
    roots
        .into_iter()
        .map(|r| r.join(rel))
        .find(|p| akantu_validation::checksum::verify_file(p, &input.sha256).is_ok())
}

/// Same rule as `safe_name` in `mesh_probe.cc`.
pub fn safe_name(n: &str) -> String {
    let s: String = n
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '/' {
                '_'
            } else {
                c
            }
        })
        .collect();
    if s.is_empty() { "_".into() } else { s }
}

/// Read the fixture's input mesh ([`mesh_input`]) with the fixture's
/// spatial dimension.
pub fn read_fixture_mesh(fx: &Fixture, opts: &ReadOptions, format: MeshIoType) -> Mesh {
    let p = &fx.manifest().parameters;
    let dim = usize::try_from(p["spatial_dimension"].as_u64().expect("spatial_dimension"))
        .expect("dimension");
    let path =
        mesh_input(fx).unwrap_or_else(|| panic!("{}: mesh input not available", fx.case_id()));
    let mut mesh = Mesh::new(dim, "mesh").expect("mesh");
    read_mesh_with(&mut mesh, &path, format, opts)
        .unwrap_or_else(|e| panic!("{}: read failed: {e}", fx.case_id()));
    mesh
}

fn type_by_name(n: &str) -> ElementType {
    ElementType::ALL
        .into_iter()
        .find(|t| t.name() == n)
        .unwrap_or_else(|| panic!("unknown element type {n}"))
}

/// Documented deviations a comparison must allow (each names its UD).
#[derive(Debug, Clone, Default)]
pub struct CompareOpts {
    /// Mesh-data quantities not compared here (checked by a dedicated test).
    pub skip_quantities: Vec<String>,
    /// UD-094: the oracle stores Diana group members under an uninitialised
    /// ghost type; compare the union of its ghost and not-ghost lists with
    /// the Rust not-ghost list.
    pub fold_ghost_groups: bool,
}

/// Compare `mesh` with everything `mesh_probe` / `mesh_io_probe` recorded
/// in the fixture at `fx_dir`. Structural problems (set mismatches) and the
/// `DiffTest` report of the numeric comparisons are returned as one error.
pub fn compare_with_fixture(test: &str, fx_dir: &Path, mesh: &Mesh) -> Result<(), String> {
    compare_with_fixture_opts(test, fx_dir, mesh, &CompareOpts::default())
}

fn i64s<T: Copy + Into<i64>>(v: &[T]) -> Vec<i64> {
    v.iter().map(|&x| x.into()).collect()
}

/// [`compare_with_fixture`] allowing the deviations in `opts`.
#[allow(clippy::too_many_lines)] // one linear checklist over the manifest
pub fn compare_with_fixture_opts(
    test: &str,
    fx_dir: &Path,
    mesh: &Mesh,
    opts: &CompareOpts,
) -> Result<(), String> {
    let fx = Fixture::open(fx_dir).map_err(|e| e.to_string())?;
    let mut problems: Vec<String> = Vec::new();
    let mut dt = DiffTest::with_fixture(test, Fixture::open(fx_dir).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let p = fx.manifest().parameters.clone();
    let ghost = GhostType::NotGhost;

    // Nodes: bit-identical (also checked through the tolerance rules).
    let nodes = mesh.nodes();
    let nodes = nodes.as_slice();
    dt.check_f64("nodes", nodes);
    if let Ok(want) = fx.load_f64("nodes")
        && (want.data.len() != nodes.len()
            || want
                .data
                .iter()
                .zip(nodes)
                .any(|(a, b)| a.to_bits() != b.to_bits()))
    {
        problems.push("nodes are not bit-identical".into());
    }
    if fx.quantity("bounds/lower").is_ok() {
        dt.check_f64("bounds/lower", &mesh.bbox().lower);
        dt.check_f64("bounds/upper", &mesh.bbox().upper);
    }

    // Element types and connectivities.
    let want_types: BTreeSet<String> = p["element_types"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|t| t["ghost_type"] == "not_ghost")
        .map(|t| t["type"].as_str().unwrap().to_owned())
        .collect();
    let have_types: BTreeSet<String> = mesh
        .element_types(TypeFilter::new().ghost(ghost))
        .map(|t| t.name().to_owned())
        .collect();
    if want_types != have_types {
        problems.push(format!(
            "element types {have_types:?}, oracle {want_types:?}"
        ));
    }
    for t in &want_types {
        if let Ok(c) = mesh.connectivity(type_by_name(t), ghost) {
            dt.check_i64(&format!("connectivity/{t}"), &i64s(c.as_slice()));
        }
    }

    // Elemental mesh data.
    let list = |key: &str| -> BTreeSet<String> {
        p[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|v| {
                        v.as_str()
                            .map_or_else(|| v["name"].as_str().unwrap().to_owned(), str::to_owned)
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let skipped = list("skipped_mesh_data");
    // `element_to_subelement` / `subelement_to_element` (upstream readers
    // end with `fillElementToSubElementsData`; the probe lists their blocks
    // as skipped element-valued data, their values are compared by the
    // `read/*` quantities of fixtures/mesh_utils): the readers produce
    // exactly upstream's blocks.
    let adjacency_names = ["element_to_subelement", "subelement_to_element"];
    let want_adjacency: BTreeSet<String> = skipped
        .iter()
        .filter(|s| {
            adjacency_names
                .iter()
                .any(|n| s.starts_with(&format!("{n}/")))
        })
        .cloned()
        .collect();
    let mut have_adjacency: BTreeSet<String> = BTreeSet::new();
    for name in adjacency_names {
        let types: Vec<ElementType> = match mesh.data().elemental(name) {
            Ok(MeshDataValues::Element(m)) => m.element_types(TypeFilter::new()).collect(),
            Ok(MeshDataValues::ElementList(m)) => m.element_types(TypeFilter::new()).collect(),
            _ => Vec::new(),
        };
        have_adjacency.extend(types.into_iter().map(|t| format!("{name}/{}", t.name())));
    }
    if want_adjacency != have_adjacency {
        problems.push(format!(
            "adjacency data blocks {have_adjacency:?}, oracle {want_adjacency:?}"
        ));
    }
    let nondet = list("nondeterministic_quantities");
    let string_tables = p["string_tables"].clone();
    let mut want_data: BTreeSet<String> = fx
        .manifest()
        .quantities
        .iter()
        .filter(|q| q.name.starts_with("mesh_data/"))
        .map(|q| q.name.clone())
        .collect();
    let mut have_data: BTreeMap<String, (String, ElementType)> = BTreeMap::new();
    for name in mesh.data().elemental_names() {
        let types: Vec<ElementType> = match mesh.data().elemental(name).unwrap() {
            MeshDataValues::Int(m) => m.element_types(TypeFilter::new()).collect(),
            MeshDataValues::String(m) => m.element_types(TypeFilter::new()).collect(),
            MeshDataValues::Real(m) => m.element_types(TypeFilter::new()).collect(),
            MeshDataValues::UInt(m) => m.element_types(TypeFilter::new()).collect(),
            MeshDataValues::Bool(m) => m.element_types(TypeFilter::new()).collect(),
            _ => Vec::new(),
        };
        for ty in types {
            have_data.insert(
                format!("mesh_data/{}/{}", safe_name(name), ty.name()),
                (name.to_owned(), ty),
            );
        }
    }
    for s in &skipped {
        want_data.remove(&format!("mesh_data/{s}"));
    }
    for q in &opts.skip_quantities {
        want_data.remove(q);
        have_data.remove(q);
    }
    for (q, (name, ty)) in &have_data {
        if nondet.contains(q) {
            continue; // UD-064: excluded from the golden data
        }
        if !want_data.remove(q) {
            problems.push(format!("extra mesh data {q}"));
            continue;
        }
        match mesh.data().elemental(name).unwrap() {
            MeshDataValues::Int(m) => dt.check_i64(q, &i64s(m.get(*ty, ghost).unwrap().as_slice())),
            MeshDataValues::UInt(m) => {
                dt.check_i64(q, &i64s(m.get(*ty, ghost).unwrap().as_slice()))
            }
            MeshDataValues::Real(m) => dt.check_f64(q, m.get(*ty, ghost).unwrap().as_slice()),
            MeshDataValues::String(m) => {
                let table: Vec<String> = string_tables[format!("{name}/{}", ty.name())]
                    .as_array()
                    .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
                    .unwrap_or_default();
                let codes = fx.load_i64(q).map(|f| f.data).unwrap_or_default();
                let want: Vec<&str> = codes
                    .iter()
                    .map(|&c| {
                        table
                            .get(usize::try_from(c).unwrap())
                            .map_or("<bad code>", String::as_str)
                    })
                    .collect();
                let have: Vec<&str> = m
                    .get(*ty, ghost)
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(String::as_str)
                    .collect();
                if want != have {
                    problems.push(format!("{q}: strings {have:?}, oracle {want:?}"));
                }
                true
            }
            other => {
                problems.push(format!("{q}: unexpected data kind {other:?}"));
                false
            }
        };
    }
    for q in want_data {
        problems.push(format!("missing mesh data {q}"));
    }
    if let Some(flags) = p["elemental_real_is_nodal"].as_object() {
        for (name, nodal) in flags {
            let skip = opts
                .skip_quantities
                .iter()
                .any(|q| q.starts_with(&format!("mesh_data/{}/", safe_name(name))));
            if !skip && mesh.data().is_elemental_nodal(name) != nodal.as_bool().unwrap() {
                problems.push(format!(
                    "dataset {name}: element-node flag differs from the oracle"
                ));
            }
        }
    }

    // Nodal data (recorded by mesh_io_probe only).
    if let Some(names) = p["nodal_data"].as_array() {
        let want: BTreeSet<String> = names
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect();
        let have: BTreeSet<String> = mesh.data().nodal_names().map(str::to_owned).collect();
        if want != have {
            problems.push(format!("nodal data {have:?}, oracle {want:?}"));
        }
        for name in want.intersection(&have) {
            match mesh.data().nodal(name) {
                Ok(NodalDataValues::Real(a)) => {
                    dt.check_f64(&format!("nodal_data/{}", safe_name(name)), a.as_slice());
                }
                _ => problems.push(format!("nodal data {name} is not real")),
            }
        }
    }

    // Node groups, through the node-group API only: standalone
    // ones and the `<name>_nodes` group of every element group (upstream
    // registers both in the same map).
    let mut have_ng: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for g in mesh.groups().node_groups() {
        if have_ng
            .insert(g.name().to_owned(), i64s(&g.node_list()))
            .is_some()
        {
            problems.push(format!("node group {} listed twice", g.name()));
        }
    }
    for g in mesh.groups().element_groups() {
        let owned = format!("{}_nodes", g.name());
        match mesh.groups().node_group(&owned) {
            Ok(ng) if ng.node_list() == g.nodes().as_slice() => {}
            Ok(_) => problems.push(format!(
                "node group {owned} differs from the nodes of element group {}",
                g.name()
            )),
            Err(e) => problems.push(format!("element group {}: {e}", g.name())),
        }
    }
    let want_ng = list("node_groups");
    if want_ng != have_ng.keys().cloned().collect() {
        problems.push(format!(
            "node groups {:?}, oracle {want_ng:?}",
            have_ng.keys().collect::<Vec<_>>()
        ));
    }
    for (name, nodes) in &have_ng {
        if want_ng.contains(name) {
            dt.check_i64(&format!("node_groups/{}", safe_name(name)), nodes);
        }
    }

    // Element groups.
    let want_eg: BTreeMap<String, serde_json::Value> = p["element_groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| (g["name"].as_str().unwrap().to_owned(), g.clone()))
        .collect();
    let have_eg: BTreeSet<String> = mesh
        .groups()
        .element_groups()
        .map(|g| g.name().to_owned())
        .collect();
    if have_eg != want_eg.keys().cloned().collect() {
        problems.push(format!(
            "element groups {have_eg:?}, oracle {:?}",
            want_eg.keys().collect::<Vec<_>>()
        ));
    }
    for g in mesh.groups().element_groups() {
        let Some(w) = want_eg.get(g.name()) else {
            continue;
        };
        let prefix = format!("element_groups/{}", safe_name(g.name()));
        // Upstream `_all_dimensions` (INT_MAX) marks a group created without
        // a dimension (Diana groups keep it even with elements, MSH groups
        // only while empty); the Rust group stores the largest element
        // dimension, 0 while empty.
        let wdim = w["dimension"].as_i64().unwrap();
        if wdim != i64::try_from(g.dimension).unwrap() && wdim != i64::from(i32::MAX) {
            problems.push(format!(
                "group {}: dimension {}, oracle {wdim}",
                g.name(),
                g.dimension
            ));
        }
        let mut want_t: BTreeSet<String> = w["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect();
        let mut folded: BTreeMap<String, Vec<i64>> = BTreeMap::new();
        if opts.fold_ghost_groups {
            let ghosts: Vec<String> = want_t
                .iter()
                .filter(|t| t.ends_with("/ghost"))
                .cloned()
                .collect();
            for t in ghosts {
                want_t.remove(&t);
                let base = t.trim_end_matches("/ghost").to_owned();
                let mut v = fx
                    .load_i64(&format!("{prefix}/{t}"))
                    .map(|f| f.data)
                    .unwrap_or_default();
                if want_t.contains(&base) {
                    v.extend(
                        fx.load_i64(&format!("{prefix}/{base}"))
                            .map(|f| f.data)
                            .unwrap_or_default(),
                    );
                }
                v.sort_unstable();
                folded.insert(base.clone(), v);
                want_t.insert(base);
            }
        }
        let have_t: BTreeSet<String> = g
            .elements
            .element_types(TypeFilter::new())
            .map(|t| t.name().to_owned())
            .collect();
        if want_t != have_t {
            problems.push(format!(
                "group {}: types {have_t:?}, oracle {want_t:?}",
                g.name()
            ));
        }
        for t in &have_t {
            let v = i64s(&g.elements_of(type_by_name(t), ghost));
            match folded.get(t) {
                Some(want) if &v != want => problems.push(format!(
                    "group {} {t}: {v:?}, oracle (ghost + not ghost) {want:?}",
                    g.name()
                )),
                Some(_) => {}
                None => {
                    dt.check_i64(&format!("{prefix}/{t}"), &v);
                }
            }
        }
        dt.check_i64(&format!("{prefix}/nodes"), &i64s(g.nodes().as_slice()));
    }

    let report = dt.finish();
    match (report, problems.is_empty()) {
        (Ok(_), true) => Ok(()),
        (Ok(_), false) => Err(problems.join("\n")),
        (Err(f), _) => Err(format!("{f}\n{}", problems.join("\n"))),
    }
}
