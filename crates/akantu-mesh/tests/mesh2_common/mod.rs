// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Shared harness of the
// Differential tests (fixtures/mesh2, oracle/probes/mesh2): case
// listing, input meshes, exact comparisons with bookkeeping of the checked
// quantities.

#![allow(dead_code)] // each test binary uses part of the harness

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use akantu_core::enums::MeshIoType;
use akantu_core::{ElementKind, ElementType, GhostType, Idx, Real};
use akantu_io::{ReadOptions, read_mesh_with};
use akantu_mesh::group::ALL_DIMENSIONS;
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::checksum::sha256_file;
use akantu_validation::{DiffTest, fixtures_root};
use serde_json::Value;

/// Case ids `mesh2/<area>/<case>`, sorted.
pub fn cases(area: &str) -> Vec<String> {
    let root = fixtures_root().join("mesh2").join(area);
    let mut out: Vec<String> = std::fs::read_dir(&root)
        .map(|d| {
            d.filter_map(|e| {
                let p = e.ok()?.path();
                let name = p.file_name()?.to_string_lossy().into_owned();
                p.join("manifest.json")
                    .exists()
                    .then(|| format!("mesh2/{area}/{name}"))
            })
            .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Fresh scratch directory below the target directory.
pub fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("akantu-mesh2-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

pub fn block_name(ty: ElementType, g: GhostType) -> String {
    if g == GhostType::Ghost {
        format!("{ty}/ghost")
    } else {
        ty.to_string()
    }
}

pub fn dim_name(d: Option<usize>) -> String {
    d.map_or_else(|| "all".to_owned(), |d| d.to_string())
}

pub fn ghost_name(g: GhostType) -> &'static str {
    if g == GhostType::Ghost {
        "ghost"
    } else {
        "not_ghost"
    }
}

pub fn kind_name(k: Option<ElementKind>) -> &'static str {
    match k {
        None => "not_defined",
        Some(ElementKind::Regular) => "regular",
        Some(ElementKind::Cohesive) => "cohesive",
        Some(ElementKind::Structural) => "structural",
        Some(_) => "other",
    }
}

/// One fixture case with bookkeeping.
pub struct Case {
    pub id: String,
    t: DiffTest,
    params: Value,
    inputs: Value,
    type_table: Vec<String>,
    quantities: Vec<String>,
    checked: BTreeSet<String>,
    problems: Vec<String>,
}

impl Case {
    pub fn open(test: &str, id: &str) -> Self {
        let t = DiffTest::new(test, id).unwrap();
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(t.fixture().dir().join("manifest.json")).unwrap(),
        )
        .unwrap();
        let params = manifest["parameters"].clone();
        let type_table = params["type_table"]
            .as_array()
            .map(|a| a.iter().map(|x| x.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default();
        let quantities = manifest["quantities"]
            .as_array()
            .unwrap()
            .iter()
            .map(|q| q["name"].as_str().unwrap().to_owned())
            .collect();
        Self {
            id: id.to_owned(),
            t,
            params,
            inputs: manifest["inputs"].clone(),
            type_table,
            quantities,
            checked: BTreeSet::new(),
            problems: Vec::new(),
        }
    }

    /// Operations of the probe spec (`ops=` list).
    pub fn ops(&self) -> BTreeSet<String> {
        let spec = self.params["spec"].as_str().unwrap();
        spec.split(';')
            .find_map(|kv| kv.strip_prefix("ops="))
            .map(|o| o.split(',').map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// Value of `key=` in the probe spec.
    pub fn spec(&self, key: &str) -> Option<String> {
        let spec = self.params["spec"].as_str().unwrap();
        spec.split(';')
            .find_map(|kv| kv.strip_prefix(&format!("{key}=")))
            .map(str::to_owned)
    }

    pub fn param(&self, key: &str) -> &Value {
        &self.params[key]
    }

    pub fn code(&self, ty: ElementType) -> i64 {
        self.type_table
            .iter()
            .position(|n| n == ty.name())
            .map_or(-99, |p| i64::try_from(p).unwrap())
    }

    pub fn type_of(&self, code: i64) -> ElementType {
        self.type_table[usize::try_from(code).unwrap()]
            .parse()
            .unwrap()
    }

    pub fn mark(&mut self, name: &str) {
        self.checked.insert(name.to_owned());
    }

    pub fn mark_param(&mut self, _name: &str) {}

    pub fn reference_i64(&self, name: &str) -> Vec<i64> {
        self.t.reference_i64(name).unwrap().data
    }

    pub fn has(&self, name: &str) -> bool {
        self.quantities.iter().any(|q| q == name)
    }

    pub fn i64(&mut self, name: &str, actual: &[i64]) {
        self.checked.insert(name.to_owned());
        if !self.t.check_i64(name, actual) {
            self.problems.push(name.to_owned());
        }
    }

    pub fn f64(&mut self, name: &str, actual: &[Real]) {
        self.checked.insert(name.to_owned());
        if !self.t.check_f64(name, actual) {
            self.problems.push(name.to_owned());
        }
    }

    /// Bytes of a file written by upstream (`uint8` quantity).
    pub fn file(&mut self, name: &str, actual: &[u8]) {
        self.checked.insert(name.to_owned());
        let want: Vec<u8> = self
            .t
            .reference_i64(name)
            .map(|f| f.data.iter().map(|&b| u8::try_from(b).unwrap()).collect())
            .unwrap_or_default();
        let as_i64: Vec<i64> = actual.iter().map(|&b| i64::from(b)).collect();
        let passed = self.t.check_i64(name, &as_i64);
        if want != actual || !passed {
            let w = String::from_utf8_lossy(&want);
            let a = String::from_utf8_lossy(actual);
            let line = w
                .lines()
                .zip(a.lines())
                .position(|(x, y)| x != y)
                .unwrap_or_else(|| w.lines().count().min(a.lines().count()));
            self.problems.push(format!(
                "{name}: files differ (first differing line {}: oracle {:?}, port {:?})",
                line + 1,
                w.lines().nth(line),
                a.lines().nth(line)
            ));
        }
    }

    /// Every file below `dir` against the recorded `files/<prefix><rel>`
    /// (same set of files, same bytes).
    pub fn tree(&mut self, prefix: &str, dir: &Path) {
        let mut rels = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    rels.push(
                        p.strip_prefix(dir)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        rels.sort();
        let want: Vec<String> = self.params["files"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str()?.strip_prefix(prefix).map(str::to_owned))
            .collect();
        if rels != want {
            self.problems.push(format!(
                "files below {prefix}: port {rels:?}, oracle {want:?}"
            ));
        }
        for r in rels {
            let bytes = std::fs::read(dir.join(&r)).unwrap();
            let q = format!("files/{prefix}{r}");
            if self.has(&q) {
                self.file(&q, &bytes);
            }
        }
    }

    pub fn eq_param(&mut self, key: &str, actual: &str) {
        if self.params[key].as_str() != Some(actual) {
            self.problems.push(format!(
                "parameter {key}: oracle {:?}, port {actual:?}",
                self.params[key]
            ));
        }
    }

    pub fn eq_param_u(&mut self, key: &str, actual: usize) {
        let want = self.params[key].as_u64();
        if want != Some(u64::try_from(actual).unwrap()) {
            self.problems.push(format!(
                "parameter {key}: oracle {:?}, port {actual}",
                self.params[key]
            ));
        }
    }

    pub fn eq_param_b(&mut self, key: &str, actual: bool) {
        if self.params[key].as_bool() != Some(actual) {
            self.problems.push(format!(
                "parameter {key}: oracle {:?}, port {actual}",
                self.params[key]
            ));
        }
    }

    pub fn eq_param_strs(&mut self, key: &str, actual: &[String]) {
        let want: Vec<String> = self.params[key]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|x| x.as_str().unwrap_or("?").to_owned())
                    .collect()
            })
            .unwrap_or_default();
        if want != actual {
            self.problems
                .push(format!("parameter {key}: oracle {want:?}, port {actual:?}"));
        }
    }

    pub fn nodes(&mut self, prefix: &str, mesh: &Mesh) {
        self.f64(&format!("{prefix}/nodes"), mesh.nodes().as_slice());
        let flags: Vec<i64> = mesh
            .nodes_flags()
            .as_slice()
            .iter()
            .map(|f| i64::from(f.bits()))
            .collect();
        self.i64(&format!("{prefix}/flags"), &flags);
        if !self.params[format!("{prefix}/nodes_id")].is_null() {
            self.eq_param(&format!("{prefix}/nodes_id"), mesh.nodes().id());
        }
    }

    pub fn connectivities(&mut self, prefix: &str, mesh: &Mesh, ids: bool) {
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            for (ty, c) in mesh.connectivities().as_map().iter(g) {
                let b = block_name(ty, g);
                let v: Vec<i64> = c
                    .read()
                    .unwrap()
                    .as_slice()
                    .iter()
                    .map(|&n| i64::from(n))
                    .collect();
                self.i64(&format!("{prefix}/connectivity/{b}"), &v);
                if ids {
                    self.eq_param(&format!("{prefix}/connectivity_id/{b}"), c.id());
                }
                blocks.push(b);
            }
        }
        self.eq_param_strs(&format!("{prefix}/blocks"), &blocks);
    }

    pub fn groups(&mut self, prefix: &str, mesh: &Mesh) {
        let want = self.params[format!("{prefix}/element_groups")].clone();
        let want = want.as_array().cloned().unwrap_or_default();
        let groups: Vec<_> = mesh.groups().element_groups().collect();
        if groups.len() != want.len() {
            self.problems.push(format!(
                "{prefix}/element_groups: oracle {} groups, port {}",
                want.len(),
                groups.len()
            ));
        }
        for (g, w) in groups.iter().zip(&want) {
            let name = g.name();
            if w["name"].as_str() != Some(name) {
                self.problems
                    .push(format!("{prefix}/element_groups: oracle {w}, port {name}"));
                continue;
            }
            let dim = if g.dimension == ALL_DIMENSIONS {
                i64::from(i32::MAX)
            } else {
                i64::try_from(g.dimension).unwrap()
            };
            if w["dimension"].as_i64() != Some(dim) {
                self.problems.push(format!(
                    "{prefix}/element_groups/{name}: dimension oracle {}, port {dim}",
                    w["dimension"]
                ));
            }
            let mut blocks = Vec::new();
            for gt in GhostType::ALL {
                for ty in g.element_types(gt) {
                    let b = block_name(ty, gt);
                    let v: Vec<i64> = g
                        .elements_of(ty, gt)
                        .iter()
                        .map(|&e| i64::from(e))
                        .collect();
                    self.i64(&format!("{prefix}/element_groups/{name}/{b}"), &v);
                    blocks.push(b);
                }
            }
            let wb: Vec<String> = w["blocks"]
                .as_array()
                .map(|a| a.iter().map(|x| x.as_str().unwrap().to_owned()).collect())
                .unwrap_or_default();
            if wb != blocks {
                self.problems.push(format!(
                    "{prefix}/element_groups/{name}: blocks oracle {wb:?}, port {blocks:?}"
                ));
            }
            let nodes: Vec<i64> = g.nodes().iter().map(|&n| i64::from(n)).collect();
            self.i64(&format!("{prefix}/element_groups/{name}/nodes"), &nodes);
        }
        let names: Vec<String> = mesh
            .groups()
            .node_groups()
            .map(|g| g.name().to_owned())
            .collect();
        self.eq_param_strs(&format!("{prefix}/node_groups"), &names);
        for g in mesh.groups().node_groups() {
            let q = format!("{prefix}/node_groups/{}", g.name());
            if self.has(&q) {
                let v: Vec<i64> = g.nodes().iter().map(|&n| i64::from(n)).collect();
                self.i64(&q, &v);
            }
        }
    }

    /// Every quantity checked, nothing failed.
    pub fn finish(self) -> Result<(), String> {
        let mut problems = self.problems;
        for q in &self.quantities {
            if !self.checked.contains(q) {
                problems.push(format!("quantity {q} was not checked"));
            }
        }
        if let Err(f) = self.t.finish() {
            problems.push(f.to_string());
        }
        if problems.is_empty() {
            Ok(())
        } else {
            Err(format!("{}:\n  {}", self.id, problems.join("\n  ")))
        }
    }

    /// Source and checksum of the input mesh.
    fn input(&self) -> Option<(String, String)> {
        let i = self
            .inputs
            .as_array()?
            .iter()
            .find(|i| i["role"] == "mesh")?;
        Some((
            i["source"].as_str()?.to_owned(),
            i["sha256"].as_str()?.to_owned(),
        ))
    }
}

/// A committed copy of upstream mesh `source` (`up:` / `fz:` path) with the
/// recorded checksum: the frozen meshes, else any fixture input of that
/// file name.
fn committed_copy(source: &str, sha: &str) -> PathBuf {
    let root = fixtures_root();
    let (kind, rel) = source.split_once(':').unwrap();
    if kind == "fz" {
        let mut frozen: Vec<PathBuf> = std::fs::read_dir(root.join("meshes"))
            .unwrap()
            .filter_map(|e| {
                let p = e.ok()?.path();
                (p.is_dir() && p.file_name()?.to_string_lossy().starts_with("gmsh-")).then_some(p)
            })
            .collect();
        frozen.sort();
        let p = frozen.last().unwrap().join(rel);
        assert_eq!(sha256_file(&p).unwrap(), sha, "{}", p.display());
        return p;
    }
    let name = Path::new(rel).file_name().unwrap().to_owned();
    let mut stack = vec![root];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.file_name() == Some(name.as_os_str())
                && sha256_file(&p).is_ok_and(|s| s == sha)
            {
                return p;
            }
        }
    }
    panic!("no committed copy of {source} (sha256 {sha})");
}

/// The probe's generated meshes (`generate()` of mesh2_probe.cc).
fn generate(mesh: &mut Mesh, name: &str) {
    let mut acc = MeshAccessor::new(mesh);
    let ng = GhostType::NotGhost;
    let push = |acc: &mut MeshAccessor<'_>, ty: ElementType, c: &[Idx]| {
        acc.connectivity_mut(ty, ng).push_tuple(c);
    };
    match name {
        "accessor_quads" => {
            acc.resize_nodes(6).unwrap();
            acc.nodes_mut()
                .as_mut_slice()
                .copy_from_slice(&[0., 0., 1., 0., 2., 0., 0., 1., 1., 1., 2., 1.]);
            acc.resize_connectivity(2, ElementType::Quadrangle4, ng)
                .unwrap();
            acc.connectivity_mut(ElementType::Quadrangle4, ng)
                .as_mut_slice()
                .copy_from_slice(&[0, 1, 4, 3, 1, 2, 5, 4]);
            acc.resize_connectivity(1, ElementType::Segment2, ng)
                .unwrap();
            acc.connectivity_mut(ElementType::Segment2, ng)
                .as_mut_slice()
                .copy_from_slice(&[0, 2]);
        }
        "cohesive_2d" => {
            for x in [[0., 0.], [1., 0.], [1., 1.], [2., 0.], [1., 0.], [1., 1.]] {
                acc.push_node(&x).unwrap();
            }
            push(&mut acc, ElementType::Triangle3, &[0, 1, 2]);
            push(&mut acc, ElementType::Triangle3, &[4, 3, 5]);
            push(&mut acc, ElementType::Cohesive2d4, &[1, 2, 4, 5]);
        }
        "mixed_2d_kinds" => {
            for x in [[0., 0.], [1., 0.], [1., 1.], [0., 1.], [2., 0.5], [3., 0.5]] {
                acc.push_node(&x).unwrap();
            }
            push(&mut acc, ElementType::Quadrangle4, &[0, 1, 2, 3]);
            push(&mut acc, ElementType::Triangle3, &[1, 4, 2]);
            push(&mut acc, ElementType::Segment2, &[4, 5]);
            push(&mut acc, ElementType::Point1, &[5]);
        }
        "beams_2d" => {
            for x in [[0., 0.], [1., 0.], [2., 1.]] {
                acc.push_node(&x).unwrap();
            }
            push(&mut acc, ElementType::BernoulliBeam2, &[0, 1]);
            push(&mut acc, ElementType::BernoulliBeam2, &[1, 2]);
        }
        other => panic!("unknown generated mesh {other}"),
    }
    acc.make_ready().unwrap();
}

/// The case's input mesh as the probe built it (id `id`).
pub fn load_mesh(c: &Case, id: &str) -> Mesh {
    let dim: usize = c.spec("dim").unwrap().parse().unwrap();
    let mut m = Mesh::new(dim, id).unwrap();
    let file = c.spec("file").unwrap();
    if let Some(name) = file.strip_prefix("gen:") {
        generate(&mut m, name);
        return m;
    }
    let (source, sha) = c.input().unwrap();
    assert_eq!(source, file);
    let path = committed_copy(&source, &sha);
    // upstream parity: unnamed MSH 4 physical groups stay empty (UD-045)
    let opts = ReadOptions {
        fill_unnamed_physical_groups: false,
        write_diana_material_file: false,
    };
    read_mesh_with(&mut m, &path, MeshIoType::Auto, &opts).unwrap();
    m
}
