// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle
// comparisons (no upstream counterpart): the probe-version-2 scenarios of
// oracle/probes/parallel_cohesive (facets-only insertion, clustering and
// fragments on a distributed mesh) recorded under the probe's names and
// compared with fixtures/parallel_cohesive/<scenario>/np<P>/rank<r>.
//
// Comparison classes: topology, groups, flags, counts, per-rank
// contributions, damage, nodal fields — bitwise (D-046). Globally summed
// floating-point values (energies, fragment integrals, MPI_Allreduce
// upstream): bitwise where every summation order of the recorded per-rank
// contributions gives the same value (always at 1 and 2 ranks); otherwise
// (order-dependent upstream) the port's value must equal the rank-order fold
// of the recorded contributions exactly (D-039 amendment), and the values
// derived from it are checked against the same derivation of the fold.
// Under ReductionPolicy::NativeMpi (real MPI, the oracle's Open MPI) every
// global value is compared bitwise.

use std::collections::BTreeMap;

use akantu_cohesive::par_scenarios::{
    self, FacetsOnlyObserver, FragmentsObserver, RunObserver, Spec, StepRecord,
};
use akantu_cohesive::{FragmentManager, MaterialCohesive, SolidMechanicsModelCohesive};
use akantu_comm::Communicator;
use akantu_core::{ElementKind, GhostType, TypeFilter};
use akantu_mesh::Mesh;
use akantu_mesh::utils::FacetFlipMode;
use akantu_validation::Fixture;

use super::oracle::{self, Level, Rec, Val};
use super::{Outcome, ok};

fn ghost_name(g: GhostType) -> &'static str {
    if g == GhostType::NotGhost {
        "not_ghost"
    } else {
        "ghost"
    }
}

fn e(r: Result<(), String>) -> akantu_core::Result<()> {
    r.map_err(akantu_core::Error::invalid)
}

/// The fixture of `scenario` for rank `r` of `np`.
fn fixture_of(scenario: &str, np: usize, r: usize) -> Result<Fixture, String> {
    Fixture::open_by_id(&format!("parallel_cohesive/{scenario}/np{np}/rank{r}"))
        .map_err(|e| e.to_string())
}

/// The partition record of rank 0 (`partition/linearized`).
fn record_partition(spec: &Spec, comm: &Communicator, rec: &mut Rec) -> Result<(), String> {
    if comm.rank() == 0 && comm.size() > 1 {
        let mut full = ok(Mesh::new(spec.dim, "full"), "mesh")?;
        ok(par_scenarios::build_mesh(spec, &mut full, true), "build")?;
        let lin: Vec<i64> = super::partition(spec, &full, comm.size())?
            .into_iter()
            .map(i64::from)
            .collect();
        rec.q.insert("partition/linearized".into(), Val::I(lin));
    }
    Ok(())
}

/// Every summation order of `values` (all pairings, any association)
/// gives the same result bitwise.
#[must_use]
pub fn order_independent(values: &[f64]) -> bool {
    fn sums(v: &[f64], out: &mut Vec<f64>) {
        if v.len() == 1 {
            out.push(v[0]);
            return;
        }
        for i in 0..v.len() {
            for j in i + 1..v.len() {
                let mut w: Vec<f64> = v
                    .iter()
                    .enumerate()
                    .filter(|&(k, _)| k != i && k != j)
                    .map(|(_, &x)| x)
                    .collect();
                w.push(v[i] + v[j]);
                sums(&w, out);
            }
        }
    }
    let mut out = Vec::new();
    sums(values, &mut out);
    out.windows(2)
        .all(|w| akantu_validation::exact::exact_eq(w[0], w[1]))
}

/// Rank-order fold of the per-rank contributions `per_rank[r][i]`.
fn fold(per_rank: &[Vec<f64>], i: usize) -> f64 {
    let mut s = per_rank[0][i];
    for r in &per_rank[1..] {
        s += r[i];
    }
    s
}

fn load_f(fx: &Fixture, name: &str) -> Result<Vec<f64>, String> {
    fx.load_f64(name).map(|x| x.data).map_err(|e| e.to_string())
}

/// The per-rank contributions `name` of every rank of `scenario` at `np`.
fn contributions(scenario: &str, np: usize, name: &str) -> Result<Vec<Vec<f64>>, String> {
    (0..np)
        .map(|r| load_f(&fixture_of(scenario, np, r)?, name))
        .collect()
}

/// Compare a globally summed array `have` (the port's, rank-ordered) with
/// the oracle's `global` and the recorded contributions `per_rank`: bitwise
/// where every summation order agrees, else equal to the rank-order fold.
/// Returns the mask of the order-dependent entries.
fn compare_sum(
    name: &str,
    have: &[f64],
    global: &[f64],
    per_rank: &[Vec<f64>],
    native: bool,
    bad: &mut Vec<String>,
) -> Vec<bool> {
    let mut masked = vec![false; have.len()];
    if have.len() != global.len() || per_rank.iter().any(|r| r.len() != have.len()) {
        bad.push(format!(
            "{name}: lengths differ (port {}, oracle {})",
            have.len(),
            global.len()
        ));
        return masked;
    }
    for i in 0..have.len() {
        let contrib: Vec<f64> = per_rank.iter().map(|r| r[i]).collect();
        if native || order_independent(&contrib) {
            if !akantu_validation::exact::exact_eq(have[i], global[i]) {
                bad.push(format!(
                    "{name}[{i}]: port {:e}, oracle {:e} (order-independent sum)",
                    have[i], global[i]
                ));
            }
        } else {
            masked[i] = true;
            let f = fold(per_rank, i);
            if !akantu_validation::exact::exact_eq(have[i], f) {
                bad.push(format!(
                    "{name}[{i}]: port {:e}, rank-order fold of the oracle's contributions {f:e}",
                    have[i]
                ));
            }
        }
    }
    masked
}

fn finish(mut bad: Vec<String>) -> Outcome {
    if bad.is_empty() {
        Ok(())
    } else {
        let n = bad.len();
        bad.truncate(25);
        Err(format!(
            "{n} difference(s) with the oracle:\n  {}",
            bad.join("\n  ")
        ))
    }
}

// ------------------------------------------------------------ facets only

struct FacetsOnlyRec<'f> {
    fx: &'f Fixture,
    rec: Rec,
    ek: Vec<f64>,
    ep: Vec<f64>,
    lek: Vec<f64>,
    lep: Vec<f64>,
    nb_steps: usize,
}

impl FacetsOnlyRec<'_> {
    fn laws(&mut self, p: &str, model: &SolidMechanicsModelCohesive) {
        let fx = self.fx;
        let p2 = p.to_owned();
        self.rec.laws(p, model, &move |m| {
            fx.manifest().parameters[format!("{p2}law{m}/cohesive_internals").as_str()]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        });
    }
}

impl RunObserver for FacetsOnlyRec<'_> {
    fn after_init(&mut self, model: &mut SolidMechanicsModelCohesive) -> akantu_core::Result<()> {
        let mesh_h = model.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        e(self.rec.topology("init/", model, &m, Level::Conn))?;
        self.laws("init/", model);
        Ok(())
    }

    fn after_step(
        &mut self,
        model: &mut SolidMechanicsModelCohesive,
        step: usize,
        record: &StepRecord,
    ) -> akantu_core::Result<()> {
        self.ek.push(record.energies[0]);
        self.ep.push(record.energies[1]);
        self.lek.push(model.smm.local_energy("kinetic")?);
        self.lep.push(model.smm.local_energy("potential")?);
        if step == self.nb_steps {
            let mesh_h = model.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            let p = format!("s{step}/");
            e(self.rec.topology(&p, model, &m, Level::Light))?;
            self.laws(&p, model);
            e(self.rec.fields(&p, model))?;
        }
        Ok(())
    }
}

impl FacetsOnlyObserver for FacetsOnlyRec<'_> {
    fn after_facets_only(
        &mut self,
        model: &mut SolidMechanicsModelCohesive,
        flagged: usize,
        result: &akantu_cohesive::inserter::InsertionResult,
    ) -> akantu_core::Result<()> {
        self.rec
            .q
            .insert("fo/flagged_local".into(), Val::I(vec![flagged as i64]));
        self.rec.q.insert(
            "fo/returned".into(),
            Val::I(vec![result.new_elements.len() as i64]),
        );
        let mesh_h = model.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        e(self.rec.topology("fo/", model, &m, Level::Full))?;
        self.laws("fo/", model);
        e(self.rec.fields("fo/", model))
    }
}

/// Run the facets-only scenario `name` on `comm` and compare it with the
/// oracle (see the module docs).
pub fn facets_only_matches_oracle(name: &str, comm: &Communicator) -> Outcome {
    let spec = par_scenarios::facets_only_specs()
        .into_iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("unknown scenario {name}"))?;
    let fx = fixture_of(name, comm.size(), comm.rank() as usize)?;
    let mesh = super::distributed(&spec, comm, FacetFlipMode::UpstreamInverse)?;
    let mut obs = FacetsOnlyRec {
        fx: &fx,
        rec: Rec::for_fixture(&fx),
        ek: Vec::new(),
        ep: Vec::new(),
        lek: Vec::new(),
        lep: Vec::new(),
        nb_steps: spec.nb_steps,
    };
    record_partition(&spec, comm, &mut obs.rec)?;
    let (_model, dt, _recs) = ok(par_scenarios::run_facets_only(&spec, mesh, &mut obs), name)?;
    obs.rec
        .q
        .insert("model/time_step".into(), Val::F(dt.to_vec()));
    obs.rec
        .q
        .insert("steps/local_energy/kinetic".into(), Val::F(obs.lek.clone()));
    obs.rec.q.insert(
        "steps/local_energy/potential".into(),
        Val::F(obs.lep.clone()),
    );
    let native = comm.reduction_policy() == akantu_comm::ReductionPolicy::NativeMpi;
    let mut bad = Vec::new();
    for prefix in [
        "partition/",
        "model/",
        "init/",
        "fo/",
        "steps/local_energy/",
        &format!("s{}/", spec.nb_steps),
    ] {
        if prefix != "partition/" || (comm.rank() == 0 && comm.size() > 1) {
            let n = fx
                .manifest()
                .quantities
                .iter()
                .filter(|q| q.name.starts_with(prefix))
                .count();
            if n == 0 {
                bad.push(format!("the oracle record has no quantity under {prefix}"));
            }
        }
        bad.extend(oracle::compare(&fx, &obs.rec, prefix));
    }
    for (what, have) in [("kinetic", &obs.ek), ("potential", &obs.ep)] {
        let global = load_f(&fx, &format!("steps/energy/{what}"))?;
        let per_rank = contributions(name, comm.size(), &format!("steps/local_energy/{what}"))?;
        // kinetic: the probe records this rank's halved sum (halving is
        // exact, so the fold of the halves is upstream's halved fold)
        let _ = compare_sum(
            &format!("steps/energy/{what}"),
            have,
            &global,
            &per_rank,
            native,
            &mut bad,
        );
    }
    finish(bad)
}

// ------------------------------------------------------------- clusters

/// Probe `record_groups` for the port's mesh groups.
fn record_groups(rec: &mut Rec, key: &str, groups: &akantu_mesh::GroupManager, prefix: &str) {
    let mut names = Vec::new();
    for g in groups.element_groups() {
        let name = g.name().to_owned();
        if !name.starts_with(prefix) {
            continue;
        }
        let mut blocks = Vec::new();
        for ghost in GhostType::ALL {
            for ty in g.group().element_types(ghost) {
                let bk = format!("{}/{}", ty.name(), ghost_name(ghost));
                rec.q.insert(
                    format!("{key}{name}/elements/{bk}"),
                    Val::I(
                        g.elements_of(ty, ghost)
                            .iter()
                            .copied()
                            .map(i64::from)
                            .collect(),
                    ),
                );
                blocks.push(bk);
            }
        }
        rec.lists.insert(format!("{key}{name}/blocks"), blocks);
        rec.params
            .insert(format!("{key}{name}/dimension"), g.group().dimension as i64);
        rec.q.insert(
            format!("{key}{name}/nodes"),
            Val::I(
                g.nodes()
                    .as_slice()
                    .iter()
                    .copied()
                    .map(i64::from)
                    .collect(),
            ),
        );
        names.push(name);
    }
    rec.lists.insert(format!("{key}names"), names);
}

/// Run the probe's `clusters_tri` on `comm` and compare it with the oracle
/// (returned counts, every cluster group's elements and nodes, all group
/// names: exact).
pub fn clusters_matches_oracle(comm: &Communicator) -> Outcome {
    let spec = par_scenarios::fragments_spec(); // the Tri3 square, 2-D rule xc = 0
    let fx = fixture_of("clusters_tri", comm.size(), comm.rank() as usize)?;
    let mut rec = Rec::for_fixture(&fx);
    let mut m = super::distributed_mesh(&spec, comm, FacetFlipMode::UpstreamInverse)?;
    let accepted = super::clusters_accepted(&m)?;
    let n = ok(
        akantu_mesh::clusters::create_clusters(
            &mut m,
            2,
            "cluster",
            &|el| accepted.contains(&el),
            akantu_mesh::clusters::ClusterFacets::Build,
        ),
        "clusters",
    )?;
    rec.q
        .insert("clusters/cluster/returned".into(), Val::I(vec![n as i64]));
    record_groups(&mut rec, "clusters/cluster/", m.groups(), "cluster_");
    let n_all = ok(
        akantu_mesh::clusters::create_clusters(
            &mut m,
            2,
            "all",
            &|_| true,
            akantu_mesh::clusters::ClusterFacets::Build,
        ),
        "clusters",
    )?;
    rec.q
        .insert("clusters/all/returned".into(), Val::I(vec![n_all as i64]));
    record_groups(&mut rec, "clusters/all/", m.groups(), "all_");
    rec.lists.insert(
        "groups/element_groups".into(),
        m.groups()
            .element_groups()
            .map(|g| g.name().to_owned())
            .collect(),
    );
    rec.lists.insert(
        "groups/node_groups".into(),
        m.groups()
            .node_groups()
            .map(|g| g.name().to_owned())
            .collect(),
    );
    record_partition(&spec, comm, &mut rec)?;
    let mut bad = Vec::new();
    for prefix in ["clusters/", "groups/", "partition/"] {
        bad.extend(oracle::compare(&fx, &rec, prefix));
    }
    if fx
        .manifest()
        .quantities
        .iter()
        .filter(|q| q.name.starts_with("clusters/"))
        .count()
        < 8
    {
        bad.push("the oracle record has fewer than 8 cluster quantities".into());
    }
    finish(bad)
}

/// Adversarial case `clusters_chk_tri`: checkerboard filter (cells with
/// `i + j` even), a group created
/// on the last rank only before the clustering; returned count, every
/// cluster group, all group names: exact.
pub fn clusters_chk_matches_oracle(comm: &Communicator) -> Outcome {
    let spec = par_scenarios::fragments_spec();
    let fx = fixture_of("clusters_chk_tri", comm.size(), comm.rank() as usize)?;
    let mut rec = Rec::for_fixture(&fx);
    let mut m = super::distributed_mesh(&spec, comm, FacetFlipMode::UpstreamInverse)?;
    if comm.rank() as usize + 1 == comm.size() {
        ok(
            m.groups_mut()
                .create_element_group("last_rank_only", 2)
                .map(|_| ()),
            "group",
        )?;
    }
    let dim = m.spatial_dimension();
    let mut accepted = std::collections::HashSet::new();
    let mut b = vec![0.0; dim];
    for g in GhostType::ALL {
        for ty in m.element_types(TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: g,
            kind: None,
        }) {
            for e in 0..m.nb_element(ty, g) {
                let el = ok(akantu_core::Element::try_new(ty, e, g), "element")?;
                ok(m.barycenter_into(el, &mut b), "barycenter")?;
                let i = ((b[0] + 1.0) / 0.25).floor() as i64;
                let j = ((b[1] + 1.0) / 0.25).floor() as i64;
                if (i + j) % 2 == 0 {
                    accepted.insert(el);
                }
            }
        }
    }
    let n = ok(
        akantu_mesh::clusters::create_clusters(
            &mut m,
            2,
            "chk",
            &|el| accepted.contains(&el),
            akantu_mesh::clusters::ClusterFacets::Build,
        ),
        "clusters",
    )?;
    rec.q
        .insert("clusters/chk/returned".into(), Val::I(vec![n as i64]));
    record_groups(&mut rec, "clusters/chk/", m.groups(), "chk_");
    rec.lists.insert(
        "groups/element_groups".into(),
        m.groups()
            .element_groups()
            .map(|g| g.name().to_owned())
            .collect(),
    );
    rec.lists.insert(
        "groups/node_groups".into(),
        m.groups()
            .node_groups()
            .map(|g| g.name().to_owned())
            .collect(),
    );
    record_partition(&spec, comm, &mut rec)?;
    let mut bad = Vec::new();
    for prefix in ["clusters/", "groups/", "partition/"] {
        bad.extend(oracle::compare(&fx, &rec, prefix));
    }
    finish(bad)
}

// ------------------------------------------------------------- fragments

/// The damage of every cohesive law (both ghost types) under `p`
/// (probe `record_damage`).
fn record_damage(
    rec: &mut Rec,
    p: &str,
    model: &SolidMechanicsModelCohesive,
) -> Result<(), String> {
    let dim = model.smm.spatial_dimension();
    for (m, law) in model.smm.physics.materials.laws().iter().enumerate() {
        let Some(c) = law
            .as_any()
            .and_then(|a| a.downcast_ref::<MaterialCohesive>())
        else {
            continue;
        };
        let field = ok(c.internal("damage"), "damage")?;
        for g in GhostType::ALL {
            for ty in field.values().element_types(TypeFilter {
                spatial_dimension: Some(dim),
                ghost_type: g,
                kind: Some(ElementKind::Cohesive),
            }) {
                let v = field
                    .get(ty, g)
                    .map(|a| a.as_slice().to_vec())
                    .unwrap_or_default();
                rec.q.insert(
                    format!("{p}law{m}/damage/{}/{}", ty.name(), ghost_name(g)),
                    Val::F(v),
                );
            }
        }
    }
    Ok(())
}

/// What a stage records: the global arrays (compared separately) and the
/// rest in `rec`.
#[derive(Default)]
struct StageGlobals {
    mass: Vec<f64>,
    center: Vec<f64>,
    velocity: Vec<f64>,
    moments: Vec<f64>,
    directions: Vec<f64>,
    local_inertia: Vec<f64>,
}

struct FragmentsRec<'f> {
    fx: &'f Fixture,
    rec: Rec,
    stages: Vec<StageGlobals>,
}

impl FragmentsObserver for FragmentsRec<'_> {
    fn after_insertion(
        &mut self,
        model: &mut SolidMechanicsModelCohesive,
        inserted: usize,
    ) -> akantu_core::Result<()> {
        self.rec
            .q
            .insert("ins/inserted".into(), Val::I(vec![inserted as i64]));
        let mesh_h = model.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        e(self.rec.topology("ins/", model, &m, Level::Light))?;
        let fx = self.fx;
        self.rec.laws("ins/", model, &move |m| {
            fx.manifest().parameters[format!("ins/law{m}/cohesive_internals").as_str()]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        });
        Ok(())
    }

    fn after_stage(
        &mut self,
        model: &mut SolidMechanicsModelCohesive,
        fm: &FragmentManager,
        stage: usize,
    ) -> akantu_core::Result<()> {
        let p = format!("frag/s{stage}/");
        let rec = &mut self.rec;
        rec.q.insert(
            format!("{p}nb_fragment"),
            Val::I(vec![fm.nb_fragment() as i64]),
        );
        rec.q.insert(
            format!("{p}nb_elements_per_fragment"),
            Val::I(
                fm.nb_elements_per_fragment
                    .iter()
                    .map(|&n| n as i64)
                    .collect(),
            ),
        );
        rec.q
            .insert(format!("{p}local/mass"), Val::F(fm.local.mass.clone()));
        rec.q.insert(
            format!("{p}local/mass_center"),
            Val::F(fm.local.mass_center.clone()),
        );
        rec.q.insert(
            format!("{p}local/velocity"),
            Val::F(fm.local.velocity.clone()),
        );
        rec.q.insert(
            format!("{p}local/nb_elements"),
            Val::I(fm.local.nb_elements.iter().map(|&n| n as i64).collect()),
        );
        e(record_damage(rec, &format!("{p}damage/"), model))?;
        // the fragment groups (upstream `FragmentManager` is a GroupManager):
        // names in name order, elements per (type, ghost) in fragment order,
        // nodes = sorted union of the elements' connectivities
        let mesh_h = model.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        let gk = format!("{p}groups/");
        let mut names: Vec<(String, usize)> = (0..fm.nb_fragment())
            .map(|i| (format!("fragment_{i}"), i))
            .collect();
        names.sort();
        for (name, i) in &names {
            let frag = &fm.fragments[*i];
            let mut blocks: BTreeMap<(u8, usize), (String, Vec<i64>)> = BTreeMap::new();
            let mut nodes = Vec::new();
            for &el in frag {
                let key = (
                    u8::from(el.ghost_type() == GhostType::Ghost),
                    el.ty().index(),
                );
                blocks
                    .entry(key)
                    .or_insert_with(|| {
                        (
                            format!("{}/{}", el.ty().name(), ghost_name(el.ghost_type())),
                            Vec::new(),
                        )
                    })
                    .1
                    .push(i64::from(el.index()));
                nodes.extend(m.element_connectivity(el)?.iter().map(|&n| i64::from(n)));
            }
            nodes.sort_unstable();
            nodes.dedup();
            let mut list = Vec::new();
            for (bk, els) in blocks.into_values() {
                rec.q
                    .insert(format!("{gk}{name}/elements/{bk}"), Val::I(els));
                list.push(bk);
            }
            rec.lists.insert(format!("{gk}{name}/blocks"), list);
            rec.params.insert(
                format!("{gk}{name}/dimension"),
                m.spatial_dimension() as i64,
            );
            rec.q.insert(format!("{gk}{name}/nodes"), Val::I(nodes));
        }
        rec.lists.insert(
            format!("{gk}names"),
            names.into_iter().map(|(n, _)| n).collect(),
        );
        self.stages.push(StageGlobals {
            mass: fm.mass.clone(),
            center: fm.mass_center.clone(),
            velocity: fm.velocity.clone(),
            moments: fm.inertia_moments.clone(),
            directions: fm.principal_directions.clone(),
            local_inertia: fm.local.inertia.clone(),
        });
        Ok(())
    }
}

/// The order-dependent global entries of `fragments_tri`
/// (those compared with the fold are not masked).
#[derive(Clone, Copy, Debug, Default)]
pub struct OrderDependent {
    /// Compared exactly with the rank-order fold of the oracle's per-rank
    /// contributions (masses, centres, velocities) or with the
    /// eigen-decomposition of that fold (inertia moments and directions).
    pub folded: usize,
    /// Not compared: the inertia moments and directions of a fragment
    /// whose centre is order-dependent (computed about another centre).
    pub skipped: usize,
}

/// Run the probe's `fragments_tri` on `comm` and compare it with the oracle
/// (see the module docs for the classes of the global arrays). Returns the
/// counts of order-dependent global entries in `Ok`.
pub fn fragments_matches_oracle(comm: &Communicator) -> Result<OrderDependent, String> {
    let spec = par_scenarios::fragments_spec();
    let np = comm.size();
    let fx = fixture_of("fragments_tri", np, comm.rank() as usize)?;
    let mesh = super::distributed(&spec, comm, FacetFlipMode::UpstreamInverse)?;
    let mut obs = FragmentsRec {
        fx: &fx,
        rec: Rec::for_fixture(&fx),
        stages: Vec::new(),
    };
    record_partition(&spec, comm, &mut obs.rec)?;
    let _ = ok(par_scenarios::run_fragments(mesh, &mut obs), "fragments")?;
    let native = comm.reduction_policy() == akantu_comm::ReductionPolicy::NativeMpi;
    let mut bad = Vec::new();
    let dim = spec.dim;
    let mut counts = OrderDependent::default();
    for (k, st) in obs.stages.iter().enumerate() {
        let p = format!("frag/s{}/", k + 1);
        for sub in [
            "nb_fragment",
            "nb_elements_per_fragment",
            "local/mass",
            "local/mass_center",
            "local/velocity",
            "local/nb_elements",
            "damage/",
            "groups/",
        ] {
            bad.extend(oracle::compare(&fx, &obs.rec, &format!("{p}{sub}")));
        }
        let c = |n: &str| contributions("fragments_tri", np, &format!("{p}local/{n}"));
        let (cm, cx, cv, ci) = (c("mass")?, c("mass_center")?, c("velocity")?, c("inertia")?);
        let gm = load_f(&fx, &format!("{p}mass"))?;
        let mass_masked = compare_sum(&format!("{p}mass"), &st.mass, &gm, &cm, native, &mut bad);
        // centres and velocities: the division of two sums
        for (what, have, num, global) in [
            (
                "center_of_mass",
                &st.center,
                &cx,
                load_f(&fx, &format!("{p}center_of_mass"))?,
            ),
            (
                "velocity",
                &st.velocity,
                &cv,
                load_f(&fx, &format!("{p}velocity"))?,
            ),
        ] {
            for i in 0..have.len().min(global.len()) {
                let nc: Vec<f64> = num.iter().map(|r| r[i]).collect();
                if !mass_masked[i] && (native || order_independent(&nc)) {
                    if !akantu_validation::exact::exact_eq(have[i], global[i]) {
                        bad.push(format!(
                            "{p}{what}[{i}]: port {:e}, oracle {:e}",
                            have[i], global[i]
                        ));
                    }
                } else {
                    counts.folded += 1;
                    let want = fold(num, i) / fold(&cm, i);
                    if !akantu_validation::exact::exact_eq(have[i], want) {
                        bad.push(format!(
                            "{p}{what}[{i}]: port {:e}, fold of the oracle's contributions {want:e}",
                            have[i]
                        ));
                    }
                }
            }
        }
        // inertia: computed about the (global) centres; comparable per
        // fragment only where its centre is order-independent
        let nf = st.mass.len() / dim;
        let gi = load_f(&fx, &format!("{p}moments_of_inertia"))?;
        let gd = load_f(&fx, &format!("{p}principal_directions"))?;
        let li = load_f(&fx, &format!("{p}local/inertia"))?;
        for f in 0..nf {
            let centre_ok = (0..dim).all(|c| {
                let i = f * dim + c;
                let mc: Vec<f64> = cm.iter().map(|r| r[i]).collect();
                let xc: Vec<f64> = cx.iter().map(|r| r[i]).collect();
                native || (order_independent(&mc) && order_independent(&xc))
            });
            if !centre_ok {
                counts.skipped += dim + dim * dim;
                continue;
            }
            let d2 = dim * dim;
            let mine = &st.local_inertia[f * d2..(f + 1) * d2];
            if !akantu_validation::exact::exact_slices_eq(mine, &li[f * d2..(f + 1) * d2]) {
                bad.push(format!(
                    "{p}local/inertia fragment {f}: differs from the oracle"
                ));
                continue;
            }
            let integrals_ok = native
                || (0..d2).all(|j| {
                    let v: Vec<f64> = ci.iter().map(|r| r[f * d2 + j]).collect();
                    order_independent(&v)
                });
            if integrals_ok {
                if !akantu_validation::exact::exact_slices_eq(
                    &st.moments[f * dim..(f + 1) * dim],
                    &gi[f * dim..(f + 1) * dim],
                ) || !akantu_validation::exact::exact_slices_eq(
                    &st.directions[f * d2..(f + 1) * d2],
                    &gd[f * d2..(f + 1) * d2],
                ) {
                    bad.push(format!(
                        "{p}moments_of_inertia / principal_directions fragment {f}: differ"
                    ));
                }
            } else {
                // order-dependent integrals: the
                // principal moments and directions of the rank-order fold
                // of the oracle's per-rank inertia integrals, with the
                // port's eigen-solver (upstream `Matrix::eig` on the
                // reduced integrals, `FragmentManager::compute_inertia_moments`)
                counts.folded += dim + d2;
                let folded: Vec<f64> = (0..d2).map(|j| fold(&ci, f * d2 + j)).collect();
                let (want_m, want_d) = ok(
                    akantu_core::tensor::MatrixX::from_col_major(dim, dim, folded).and_then(|a| {
                        akantu_linalg::dense::eig_vectors(
                            &a,
                            akantu_linalg::dense::Storage::Dynamic,
                            akantu_linalg::dense::SortOrder::Descending,
                        )
                    }),
                    "eigen-decomposition of the folded inertia",
                )?;
                if !akantu_validation::exact::exact_slices_eq(
                    &st.moments[f * dim..(f + 1) * dim],
                    &want_m,
                ) || !akantu_validation::exact::exact_slices_eq(
                    &st.directions[f * d2..(f + 1) * d2],
                    want_d.as_slice(),
                ) {
                    bad.push(format!(
                        "{p}moments_of_inertia / principal_directions fragment {f}: differ \
                         from the eigen-decomposition of the fold of the oracle's contributions"
                    ));
                }
            }
        }
        counts.folded += mass_masked.iter().filter(|&&m| m).count();
    }
    for prefix in ["ins/", "partition/"] {
        bad.extend(oracle::compare(&fx, &obs.rec, prefix));
    }
    finish(bad).map(|()| counts)
}
