// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). oracle
// comparison (no upstream counterpart): the port's state recorded under the
// names of oracle/probes/parallel_cohesive/parallel_cohesive_probe.cc and
// compared with the rank's fixture fixtures/parallel_cohesive/<scenario>/
// np<P>/rank<r>, quantity by quantity, bitwise (D-046); every quantity of a
// compared prefix must exist on both sides.

use std::collections::BTreeMap;

use akantu_cohesive::{MaterialCohesive, SolidMechanicsModelCohesive};
use akantu_core::{Element, ElementType, ElementTypeMapArray, GhostType, Idx, TypeFilter};
use akantu_mesh::Mesh;
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT};
use akantu_validation::Fixture;

/// A recorded quantity.
#[derive(Clone, Debug)]
pub enum Val {
    /// Integer data.
    I(Vec<i64>),
    /// Floating-point data.
    F(Vec<f64>),
}

/// The port's record of one rank (quantities, string lists and integer
/// parameters under the probe's names).
#[derive(Debug, Default)]
pub struct Rec {
    pub q: BTreeMap<String, Val>,
    pub lists: BTreeMap<String, Vec<String>>,
    pub params: BTreeMap<String, i64>,
    /// Type codes of the fixture's `type_table` (element triples).
    pub codes: BTreeMap<&'static str, i64>,
}

/// Checkpoint record levels (probe `Level`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    /// Nodes, schemes, check / insertion facets.
    Light,
    /// + connectivities.
    Conn,
    /// + facet-mesh adjacency.
    Full,
}

fn ghost_name(g: GhostType) -> &'static str {
    if g == GhostType::NotGhost {
        "not_ghost"
    } else {
        "ghost"
    }
}

impl Rec {
    /// A record whose element triples use the type table of `fx`.
    pub fn for_fixture(fx: &Fixture) -> Self {
        let mut r = Rec::default();
        if let Some(a) = fx.manifest().parameters["type_table"].as_array() {
            for (i, v) in a.iter().enumerate() {
                if let Some(name) = v.as_str()
                    && let Some(ty) = ElementType::ALL.iter().find(|t| t.name() == name)
                {
                    r.codes.insert(ty.name(), i as i64);
                }
            }
        }
        r
    }

    fn i(&mut self, k: String, v: Vec<i64>) {
        self.q.insert(k, Val::I(v));
    }

    fn f(&mut self, k: String, v: Vec<f64>) {
        self.q.insert(k, Val::F(v));
    }

    fn push_el(&self, v: &mut Vec<i64>, e: Element) -> Result<(), String> {
        if e.is_null() {
            v.extend([-1, -1, -1]);
            return Ok(());
        }
        let code = *self
            .codes
            .get(e.ty().name())
            .ok_or_else(|| format!("type {} not in the fixture's type table", e.ty().name()))?;
        v.extend([
            code,
            e.index() as i64,
            i64::from(e.ghost_type() == GhostType::Ghost),
        ]);
        Ok(())
    }

    /// Probe `record_nodes`.
    pub fn nodes(&mut self, p: &str, m: &Mesh) {
        let n = m.nb_nodes();
        self.i(
            format!("{p}nodes/global_ids"),
            (0..n).map(|i| m.node_global_id(i as Idx)).collect(),
        );
        self.i(
            format!("{p}nodes/flags"),
            m.nodes_flags()
                .as_slice()
                .iter()
                .map(|f| i64::from(f.bits()))
                .collect(),
        );
        self.i(
            format!("{p}nodes/pranks"),
            (0..n).map(|i| i64::from(m.node_prank(i as Idx))).collect(),
        );
        self.f(
            format!("{p}nodes/coordinates"),
            m.nodes().as_slice().to_vec(),
        );
        self.params
            .insert(format!("{p}nb_global_nodes"), m.nb_global_nodes() as i64);
        self.params.insert(format!("{p}nb_nodes"), n as i64);
    }

    /// Probe `record_connectivities`.
    pub fn connectivities(&mut self, p: &str, m: &Mesh) -> Result<(), String> {
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            for ty in m.element_types(TypeFilter::new().ghost(g)) {
                let key = format!("{}/{}", ty.name(), ghost_name(g));
                let c = m.connectivity(ty, g).map_err(|e| e.to_string())?;
                self.i(
                    format!("{p}{key}"),
                    c.as_slice().iter().map(|&x| i64::from(x)).collect(),
                );
                blocks.push(key);
            }
        }
        self.lists.insert(format!("{p}blocks"), blocks);
        Ok(())
    }

    /// Probe `record_adjacency` (facet mesh).
    pub fn adjacency(&mut self, p: &str, fm: &Mesh) -> Result<(), String> {
        let e2s = fm
            .data()
            .element_lists(ELEMENT_TO_SUBELEMENT)
            .map_err(|e| e.to_string())?;
        let s2e = fm
            .data()
            .element_map(SUBELEMENT_TO_ELEMENT)
            .map_err(|e| e.to_string())?;
        let mut e2s_blocks = Vec::new();
        let mut s2e_blocks = Vec::new();
        for g in GhostType::ALL {
            for (ty, a) in e2s.as_map().iter(g) {
                let mut flat = Vec::new();
                let mut offsets = vec![0i64];
                for row in a.as_slice() {
                    for &e in row {
                        self.push_el(&mut flat, e)?;
                    }
                    offsets.push((flat.len() / 3) as i64);
                }
                let key = format!("{}/{}", ty.name(), ghost_name(g));
                self.i(format!("{p}element_to_subelement/{key}"), flat);
                self.i(format!("{p}element_to_subelement_offsets/{key}"), offsets);
                e2s_blocks.push(key);
            }
            for (ty, a) in s2e.as_map().iter(g) {
                let mut flat = Vec::new();
                for &e in a.as_slice() {
                    self.push_el(&mut flat, e)?;
                }
                let key = format!("{}/{}", ty.name(), ghost_name(g));
                self.i(format!("{p}subelement_to_element/{key}"), flat);
                self.params.insert(
                    format!("{p}subelement_to_element_components/{key}"),
                    a.nb_component() as i64,
                );
                s2e_blocks.push(key);
            }
        }
        self.lists
            .insert(format!("{p}element_to_subelement_blocks"), e2s_blocks);
        self.lists
            .insert(format!("{p}subelement_to_element_blocks"), s2e_blocks);
        Ok(())
    }

    /// Probe `record_schemes` (element synchronizers).
    pub fn schemes(
        &mut self,
        p: &str,
        s: &akantu_comm::Synchronizer<Element>,
    ) -> Result<(), String> {
        let mut keys = Vec::new();
        for (dir, map) in [
            ("send", &s.communications.send),
            ("recv", &s.communications.recv),
        ] {
            for (proc, scheme) in map {
                let mut v = Vec::new();
                for &e in scheme {
                    self.push_el(&mut v, e)?;
                }
                let key = format!("{dir}/{proc}");
                self.i(format!("{p}{key}"), v);
                keys.push(key);
            }
        }
        self.lists.insert(format!("{p}keys"), keys);
        Ok(())
    }

    /// Probe `record_node_schemes`.
    pub fn node_schemes(&mut self, p: &str, s: &akantu_comm::Synchronizer<Idx>) {
        let mut keys = Vec::new();
        for (dir, map) in [
            ("send", &s.communications.send),
            ("recv", &s.communications.recv),
        ] {
            for (proc, scheme) in map {
                let key = format!("{dir}/{proc}");
                self.i(
                    format!("{p}{key}"),
                    scheme.iter().map(|&x| i64::from(x)).collect(),
                );
                keys.push(key);
            }
        }
        self.lists.insert(format!("{p}keys"), keys);
    }

    /// Probe `record_bool_map`.
    pub fn bool_map(&mut self, p: &str, m: &ElementTypeMapArray<bool>) {
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            for (ty, a) in m.as_map().iter(g) {
                let key = format!("{}/{}", ty.name(), ghost_name(g));
                self.i(
                    format!("{p}{key}"),
                    a.as_slice().iter().map(|&b| i64::from(b)).collect(),
                );
                blocks.push(key);
            }
        }
        self.lists.insert(format!("{p}blocks"), blocks);
    }

    /// Probe `record_topology`.
    pub fn topology(
        &mut self,
        p: &str,
        model: &SolidMechanicsModelCohesive,
        m: &Mesh,
        level: Level,
    ) -> Result<(), String> {
        self.nodes(p, m);
        let fm = m.mesh_facets().ok_or("no facet mesh")?;
        if level != Level::Light {
            self.connectivities(&format!("{p}conn/"), m)?;
            self.connectivities(&format!("{p}facets/conn/"), fm)?;
        }
        if level == Level::Full {
            self.adjacency(&format!("{p}facets/"), fm)?;
        }
        if m.is_distributed() {
            if let Some(es) = &m.synchronizers().element {
                self.schemes(&format!("{p}element_scheme/"), es)?;
            }
            if let Some(ns) = &m.synchronizers().node {
                self.node_schemes(&format!("{p}node_scheme/"), ns);
            }
            if let Some(fs) = &fm.synchronizers().element {
                self.schemes(&format!("{p}facet_scheme/"), fs)?;
            }
            if let Some(par) = &model.ext.parallel {
                self.schemes(&format!("{p}cohesive_scheme/"), &par.cohesive)?;
                if let Some(fss) = &par.facet_stress {
                    self.schemes(&format!("{p}facet_stress_scheme/"), fss)?;
                }
            }
        }
        self.bool_map(
            &format!("{p}check_facets/"),
            model.ext.inserter.check_facets(),
        );
        self.bool_map(
            &format!("{p}insertion_facets/"),
            model.ext.inserter.insertion_facets(),
        );
        Ok(())
    }

    /// Probe `record_laws`.
    pub fn laws(
        &mut self,
        p: &str,
        model: &SolidMechanicsModelCohesive,
        internals: &dyn Fn(usize) -> Vec<String>,
    ) {
        let dim = model.smm.spatial_dimension();
        let mut names = Vec::new();
        for (m, law) in model.smm.physics.materials.laws().iter().enumerate() {
            names.push(law.name().to_owned());
            let lp = format!("{p}law{m}/");
            for g in GhostType::ALL {
                for (ty, a) in law.base().element_filter.as_map().iter(g) {
                    self.i(
                        format!("{lp}filter/{}/{}", ty.name(), ghost_name(g)),
                        a.as_slice().iter().map(|&x| i64::from(x)).collect(),
                    );
                }
            }
            let Some(c) = law
                .as_any()
                .and_then(|a| a.downcast_ref::<MaterialCohesive>())
            else {
                continue;
            };
            let recorded = internals(m);
            for internal in &recorded {
                let Ok(field) = c.internal(internal) else {
                    continue;
                };
                for g in GhostType::ALL {
                    for ty in field.values().element_types(TypeFilter {
                        spatial_dimension: Some(dim),
                        ghost_type: g,
                        kind: Some(akantu_core::ElementKind::Cohesive),
                    }) {
                        let v = field
                            .get(ty, g)
                            .map(|a| a.as_slice().to_vec())
                            .unwrap_or_default();
                        self.f(
                            format!(
                                "{lp}{}/{}/{}",
                                internal.replace(' ', "_"),
                                ty.name(),
                                ghost_name(g)
                            ),
                            v,
                        );
                    }
                }
            }
            self.lists
                .insert(format!("{lp}cohesive_internals"), recorded.clone());
            for g in GhostType::ALL {
                for (ty, a) in c.core.facet_filter.as_map().iter(g) {
                    self.i(
                        format!("{lp}facet_filter/{}/{}", ty.name(), ghost_name(g)),
                        a.as_slice().iter().map(|&x| i64::from(x)).collect(),
                    );
                }
            }
            for g in GhostType::ALL {
                for ty in c.core.sigma_c.values().element_types(TypeFilter {
                    spatial_dimension: Some(dim - 1),
                    ghost_type: g,
                    kind: Some(akantu_core::ElementKind::Regular),
                }) {
                    let v = c
                        .core
                        .sigma_c
                        .get(ty, g)
                        .map(|a| a.as_slice().to_vec())
                        .unwrap_or_default();
                    self.f(format!("{lp}sigma_c/{}/{}", ty.name(), ghost_name(g)), v);
                }
            }
        }
        self.lists.insert(format!("{p}law_names"), names);
    }

    /// Probe `record_facet_material`.
    pub fn facet_material(&mut self, p: &str, model: &SolidMechanicsModelCohesive) {
        for g in GhostType::ALL {
            for (ty, a) in model.ext.facet_material.as_map().iter(g) {
                self.i(
                    format!("{p}{}/{}", ty.name(), ghost_name(g)),
                    a.as_slice().iter().map(|&x| i64::from(x)).collect(),
                );
            }
        }
    }

    /// Probe `record_fields`.
    pub fn fields(&mut self, p: &str, model: &SolidMechanicsModelCohesive) -> Result<(), String> {
        let e = |r: akantu_core::Result<Vec<f64>>| r.map_err(|e| e.to_string());
        self.f(
            format!("{p}model/displacement"),
            e(model.smm.displacement().map(|a| a.as_slice().to_vec()))?,
        );
        self.f(
            format!("{p}model/velocity"),
            e(model.smm.velocity().map(|a| a.as_slice().to_vec()))?,
        );
        self.f(
            format!("{p}model/acceleration"),
            e(model.smm.acceleration().map(|a| a.as_slice().to_vec()))?,
        );
        self.f(
            format!("{p}model/internal_force"),
            e(model.smm.internal_force().map(|a| a.as_slice().to_vec()))?,
        );
        self.f(
            format!("{p}model/mass"),
            e(model.smm.mass().map(|a| a.as_slice().to_vec()))?,
        );
        self.i(
            format!("{p}model/blocked_dofs"),
            model
                .smm
                .blocked_dofs()
                .map_err(|e| e.to_string())?
                .as_slice()
                .iter()
                .map(|&b| i64::from(b))
                .collect(),
        );
        Ok(())
    }

    /// Probe `record_facet_stress` over `selected`.
    pub fn facet_stress(
        &mut self,
        p: &str,
        model: &SolidMechanicsModelCohesive,
        fm: &Mesh,
        selected: &ElementTypeMapArray<bool>,
    ) {
        let dim = fm.spatial_dimension();
        let mut blocks = Vec::new();
        for g in GhostType::ALL {
            for ty in fm.element_types(TypeFilter {
                spatial_dimension: Some(dim - 1),
                ghost_type: g,
                kind: Some(akantu_core::ElementKind::Regular),
            }) {
                let nf = fm.nb_element(ty, g);
                let Ok(sel) = selected.get(ty, g) else {
                    continue;
                };
                if nf == 0 {
                    continue;
                }
                let Ok(fs) = model.ext.facet_stress.get(ty, g) else {
                    continue;
                };
                let nbc = fs.nb_component();
                let nq = fs.len() / nf.max(1);
                let mut idx = Vec::new();
                let mut vals = Vec::new();
                for (f, &s) in sel.as_slice().iter().enumerate() {
                    if !s {
                        continue;
                    }
                    idx.push(f as i64);
                    vals.extend_from_slice(&fs.as_slice()[f * nq * nbc..(f + 1) * nq * nbc]);
                }
                let key = format!("{}/{}", ty.name(), ghost_name(g));
                self.i(format!("{p}facets/{key}"), idx);
                self.f(format!("{p}values/{key}"), vals);
                self.params.insert(format!("{p}nb_quad/{key}"), nq as i64);
                blocks.push(key);
            }
        }
        self.lists.insert(format!("{p}blocks"), blocks);
    }
}

/// Number of values of the oracle quantity `name` (0 if absent).
fn oracle_len(fx: &Fixture, name: &str) -> Option<usize> {
    fx.quantity(name).ok().map(|q| q.shape.iter().product())
}

fn port_len(rec: &Rec, name: &str) -> Option<usize> {
    rec.q.get(name).map(|v| match v {
        Val::I(x) => x.len(),
        Val::F(x) => x.len(),
    })
}

/// The quantity-name base of a block list (`…/blocks` → `…/`,
/// `…_blocks` → `…/`, `…/keys` → `…/`); `None` for other lists.
fn list_base(key: &str) -> Option<String> {
    if let Some(b) = key.strip_suffix("/blocks") {
        Some(format!("{b}/"))
    } else if let Some(b) = key.strip_suffix("_blocks") {
        Some(format!("{b}/"))
    } else {
        key.strip_suffix("/keys").map(|b| format!("{b}/"))
    }
}

/// Compare every quantity, list and integer parameter of `fx` whose name
/// starts with `prefix` with `rec` (bitwise; D-046) and the reverse
/// (quantities the port records under `prefix` must exist in the oracle).
///
/// Convention: an **absent block equals an empty one** (upstream allocates
/// many `ElementTypeMapArray` blocks eagerly, e.g. an empty `_triangle_3`
/// block in a cohesive law's filter or an empty ghost
/// `subelement_to_element` block of a cohesive type; the port allocates
/// lazily; an empty block holds no state): a quantity with no values on one
/// side and absent on the other is equal, and block lists are compared
/// without their empty blocks. Returns the differences.
pub fn compare(fx: &Fixture, rec: &Rec, prefix: &str) -> Vec<String> {
    let mut bad = Vec::new();
    for q in &fx.manifest().quantities {
        let name = &q.name;
        if !name.starts_with(prefix) {
            continue;
        }
        match rec.q.get(name) {
            None if oracle_len(fx, name) == Some(0) => {}
            None => bad.push(format!("{name}: recorded by the oracle, not by the port")),
            Some(Val::I(v)) => match fx.load_i64(name) {
                Ok(f) if f.data == *v => {}
                Ok(f) => bad.push(format!(
                    "{name}: differs (port {} values, oracle {}){}",
                    v.len(),
                    f.data.len(),
                    first_diff_i(v, &f.data)
                )),
                Err(e) => bad.push(format!("{name}: {e}")),
            },
            Some(Val::F(v)) => match fx.load_f64(name) {
                Ok(f) if akantu_validation::exact::exact_slices_eq(&f.data, v) => {}
                Ok(f) => bad.push(format!(
                    "{name}: differs (port {} values, oracle {}){}",
                    v.len(),
                    f.data.len(),
                    first_diff_f(v, &f.data)
                )),
                Err(e) => bad.push(format!("{name}: {e}")),
            },
        }
    }
    for name in rec.q.keys() {
        if name.starts_with(prefix) && fx.quantity(name).is_err() && port_len(rec, name) != Some(0)
        {
            bad.push(format!("{name}: recorded by the port, not by the oracle"));
        }
    }
    let p = &fx.manifest().parameters;
    for (k, list) in &rec.lists {
        if !k.starts_with(prefix) {
            continue;
        }
        let mut want: Vec<String> = p[k.as_str()]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let mut have = list.clone();
        if let Some(base) = list_base(k) {
            want.retain(|b| oracle_len(fx, &format!("{base}{b}")) != Some(0));
            have.retain(|b| port_len(rec, &format!("{base}{b}")) != Some(0));
        }
        if want != have {
            bad.push(format!("{k}: port {have:?}, oracle {want:?}"));
        }
    }
    for (k, v) in &rec.params {
        if !k.starts_with(prefix) {
            continue;
        }
        if p[k.as_str()].as_i64() != Some(*v) {
            bad.push(format!("{k}: port {v}, oracle {}", p[k.as_str()]));
        }
    }
    bad
}

fn first_diff_i(a: &[i64], b: &[i64]) -> String {
    a.iter()
        .zip(b)
        .position(|(x, y)| x != y)
        .map_or(String::new(), |i| {
            format!(", first at {i}: {} vs {}", a[i], b[i])
        })
}

fn first_diff_f(a: &[f64], b: &[f64]) -> String {
    a.iter()
        .zip(b)
        .position(|(x, y)| !akantu_validation::exact::exact_eq(*y, *x))
        .map_or(String::new(), |i| {
            format!(", first at {i}: {:e} vs {:e}", a[i], b[i])
        })
}
