// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). test
// support (no upstream counterpart): the parallel cohesive scenarios of
// oracle/probes/parallel_cohesive on a distributed mesh, shared by the
// local-world tests (parallel_cohesive.rs) and the real-MPI binary
// (mpi_cohesive.rs).

#![allow(dead_code)] // shared by several test binaries

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::par_scenarios::{self, RunObserver, Spec, StepRecord};
use akantu_comm::Communicator;
use akantu_core::{Idx, Real};
use akantu_mesh::facet_synchronizer::DistributedFacetsOptions;
use akantu_mesh::partition::{MeshPartitioner, PrescribedPartition};
use akantu_mesh::utils::{FacetFlipMode, init_mesh_facets_with};
use akantu_mesh::{Mesh, SharedMesh};

/// Per-operation timeout of the local world (`AKANTU_PAR_COH_TIMEOUT_S`,
/// default 300 s; the mutation campaign uses a short one: a mutant that
/// desynchronises the schemes makes a rank wait for a message that never
/// comes).
pub fn local_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(
        std::env::var("AKANTU_PAR_COH_TIMEOUT_S")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300),
    )
}

/// Outcome of a scenario on one rank.
pub type Outcome = std::result::Result<(), String>;

/// `r` or its error as a string with context.
pub fn ok<T>(r: akantu_core::Result<T>, what: &str) -> std::result::Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

/// The probe's 2-D partition rule at 8 ranks (probe version 4;
/// `oracle/probes/parallel_cohesive` header): four strips on
/// each side of the crack `x = xc`, the right ones shifted by a quarter of a
/// strip, and the cells `(ic, 4)` and `(ic, 6)` of the 4-rank rule on rank
/// 0. Test-side on purpose: the library's [`par_scenarios::part_2d`] keeps
/// the rules of 1, 2 and 4 ranks (no library change for the 8-rank cases);
/// the port computes this partition itself and compares it with the
/// recorded `partition/linearized`.
pub fn part_2d_8(b: &[Real], xc: Real) -> Idx {
    let h = 0.25;
    #[allow(clippy::cast_possible_truncation)] // the probe's Int(std::floor(...)), small
    let cell = |x: Real| ((x + 1.0) / h).floor() as i64;
    #[allow(clippy::cast_possible_truncation)] // the probe's std::lround
    let ic = ((xc + 1.0) / h).round() as i64;
    let (i, j) = (cell(b[0]), cell(b[1]));
    if b[0] <= xc {
        return if b[1] < -0.5 {
            0
        } else if b[1] < 0.0 {
            1
        } else if b[1] < 0.5 {
            2
        } else {
            3
        };
    }
    if i == ic && (j == 4 || j == 6) {
        return 0;
    }
    if b[1] < -0.75 {
        4
    } else if b[1] < -0.25 {
        5
    } else if b[1] < 0.25 {
        6
    } else {
        7
    }
}

/// Linearised parts of `mesh` at `np` ranks: the library's rules at 1, 2
/// and 4 ranks ([`par_scenarios::partition`]), [`part_2d_8`] for a 2-D
/// scenario at 8 ranks.
pub fn partition(spec: &Spec, mesh: &Mesh, np: usize) -> std::result::Result<Vec<Idx>, String> {
    if np != 8 || spec.dim != 2 {
        return ok(par_scenarios::partition(spec, mesh, np), "partition");
    }
    let mut out = Vec::new();
    let mut b = vec![0.0; 2];
    for ty in mesh.element_types(TypeFilter {
        spatial_dimension: Some(2),
        ghost_type: GhostType::NotGhost,
        kind: None,
    }) {
        for e in 0..mesh.nb_element(ty, GhostType::NotGhost) {
            let el = ok(
                akantu_core::Element::try_new(ty, e, GhostType::NotGhost),
                "element",
            )?;
            ok(mesh.barycenter_into(el, &mut b), "barycenter")?;
            out.push(part_2d_8(&b, spec.crack_x));
        }
    }
    Ok(out)
}

/// Rank counts at which the oracle recorded `scenario`
/// (`fixtures/parallel_cohesive/<scenario>/np<P>`), sorted.
pub fn oracle_rank_counts(scenario: &str) -> Vec<usize> {
    let root = akantu_validation::fixtures_root()
        .join("parallel_cohesive")
        .join(scenario);
    let mut out: Vec<usize> = std::fs::read_dir(&root)
        .map(|d| {
            d.filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str()?.strip_prefix("np")?.parse().ok())
                .collect()
        })
        .unwrap_or_default();
    out.sort_unstable();
    out
}

/// The scenario's mesh distributed over `comm` with the probe's partition
/// rule, and its facet mesh built collectively with the flip `flip`
/// (`UpstreamInverse` reproduces the oracle's ghost facets, UD-122).
pub fn distributed(
    spec: &Spec,
    comm: &Communicator,
    flip: FacetFlipMode,
) -> std::result::Result<SharedMesh, String> {
    distributed_mesh(spec, comm, flip).map(SharedMesh::new)
}

/// [`distributed`] before it is shared.
pub fn distributed_mesh(
    spec: &Spec,
    comm: &Communicator,
    flip: FacetFlipMode,
) -> std::result::Result<Mesh, String> {
    let mut m = ok(
        Mesh::with_communicator(spec.dim, "mesh", comm.clone()),
        "mesh",
    )?;
    let root = comm.rank() == 0;
    ok(par_scenarios::build_mesh(spec, &mut m, root), "build")?;
    let parts: Vec<Idx> = if root {
        partition(spec, &m, comm.size())?
    } else {
        Vec::new()
    };
    let mut p = PrescribedPartition::new(parts);
    let _ = ok(
        akantu_mesh::distribution::distribute_with(
            &mut m,
            Some(&mut p as &mut dyn MeshPartitioner),
            0,
        ),
        "distribute",
    )?;
    let _ = ok(
        init_mesh_facets_with(&mut m, DistributedFacetsOptions { flip }),
        "init_mesh_facets",
    )?;
    Ok(m)
}

/// The elements of the mesh dimension (local and ghost) accepted by the
/// probe's `clusters_tri` filter ([`par_scenarios::clusters_filter`] on
/// their barycentre).
pub fn clusters_accepted(
    m: &Mesh,
) -> std::result::Result<std::collections::HashSet<akantu_core::Element>, String> {
    let dim = m.spatial_dimension();
    let mut out = std::collections::HashSet::new();
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
                if par_scenarios::clusters_filter(&b) {
                    out.insert(el);
                }
            }
        }
    }
    Ok(out)
}

/// Run `spec` on `comm` (distributed with the upstream flip) with `observer`.
pub fn run(
    spec: &Spec,
    comm: &Communicator,
    observer: &mut dyn RunObserver,
) -> std::result::Result<
    (
        SolidMechanicsModelCohesive,
        SharedMesh,
        [Real; 2],
        Vec<StepRecord>,
    ),
    String,
> {
    let mesh = distributed(spec, comm, FacetFlipMode::UpstreamInverse)?;
    let (model, dt, recs) = ok(par_scenarios::run(spec, mesh.clone(), observer), spec.name)?;
    Ok((model, mesh, dt, recs))
}

pub mod coh2;
pub mod oracle;

use akantu_core::{ElementKind, ElementTypeMapArray, GhostType, TypeFilter};
use akantu_validation::Fixture;
use oracle::{Level, Rec};

/// The rank's oracle record of `scenario`.
pub fn fixture(scenario: &str, comm: &Communicator) -> std::result::Result<Fixture, String> {
    Fixture::open_by_id(&format!(
        "parallel_cohesive/{scenario}/np{}/rank{}",
        comm.size(),
        comm.rank()
    ))
    .map_err(|e| e.to_string())
}

/// Observer recording the port's state at the probe's checkpoints.
pub struct OracleObserver<'f> {
    pub spec: Spec,
    pub fx: &'f Fixture,
    pub rec: Rec,
    pub checked_at_init: Option<ElementTypeMapArray<bool>>,
    pub first_insertion: Option<usize>,
    pub nb_coh: Vec<(i64, i64)>,
    pub local_energies: Vec<[f64; 5]>,
}

fn internals_of(fx: &Fixture, prefix: &str, m: usize) -> Vec<String> {
    fx.manifest().parameters[format!("{prefix}law{m}/cohesive_internals").as_str()]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn cohesive_counts(m: &Mesh, dim: usize) -> (i64, i64) {
    let count = |g: GhostType| -> i64 {
        m.element_types(TypeFilter {
            spatial_dimension: Some(dim),
            ghost_type: g,
            kind: Some(ElementKind::Cohesive),
        })
        .map(|t| m.nb_element(t, g) as i64)
        .sum()
    };
    (count(GhostType::NotGhost), count(GhostType::Ghost))
}

impl<'f> OracleObserver<'f> {
    pub fn new(spec: &Spec, fx: &'f Fixture) -> Self {
        Self {
            spec: spec.clone(),
            fx,
            rec: Rec::for_fixture(fx),
            checked_at_init: None,
            first_insertion: None,
            nb_coh: Vec::new(),
            local_energies: Vec::new(),
        }
    }

    fn laws(&mut self, prefix: &str, model: &SolidMechanicsModelCohesive) {
        let fx = self.fx;
        let p = prefix.to_owned();
        self.rec
            .laws(prefix, model, &move |m| internals_of(fx, &p, m));
    }
}

fn e(r: std::result::Result<(), String>) -> akantu_core::Result<()> {
    r.map_err(akantu_core::Error::invalid)
}

impl RunObserver for OracleObserver<'_> {
    fn after_init(&mut self, model: &mut SolidMechanicsModelCohesive) -> akantu_core::Result<()> {
        let mesh_h = model.smm.physics.base.mesh.clone();
        let m = mesh_h.read()?;
        let level = if self.spec.extrinsic {
            Level::Conn
        } else {
            Level::Full
        };
        e(self.rec.topology("init/", model, &m, level))?;
        self.laws("init/", model);
        self.rec.facet_material("init/facet_material/", model);
        self.checked_at_init = Some(model.ext.inserter.check_facets().clone());
        Ok(())
    }

    fn after_check(
        &mut self,
        model: &mut SolidMechanicsModelCohesive,
        step: usize,
        inserted: usize,
    ) -> akantu_core::Result<()> {
        if inserted > 0 && self.first_insertion.is_none() {
            self.first_insertion = Some(step);
            let mesh_h = model.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            let fm = m
                .mesh_facets()
                .ok_or_else(|| akantu_core::Error::Mesh("no facet mesh".into()))?;
            if let Some(sel) = &self.checked_at_init {
                self.rec.facet_stress("ins1/facet_stress/", model, fm, sel);
            }
            e(self.rec.topology("ins1/", model, &m, Level::Light))?;
            self.laws("ins1/", model);
            self.rec.facet_material("ins1/facet_material/", model);
        }
        Ok(())
    }

    fn after_step(
        &mut self,
        model: &mut SolidMechanicsModelCohesive,
        step: usize,
        _record: &StepRecord,
    ) -> akantu_core::Result<()> {
        {
            let mesh_h = model.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            self.nb_coh.push(cohesive_counts(&m, self.spec.dim));
        }
        let mut le = [0.0; 5];
        for (v, id) in le.iter_mut().zip([
            "kinetic",
            "potential",
            "dissipated",
            "reversible",
            "cohesive contact",
        ]) {
            *v = model.smm.local_energy(id)?;
        }
        self.local_energies.push(le);
        if self.spec.checkpoints.contains(&step) {
            let mesh_h = model.smm.physics.base.mesh.clone();
            let m = mesh_h.read()?;
            let prefix = format!("s{step}/");
            let level = if self.spec.extrinsic && step == self.spec.nb_steps {
                Level::Full
            } else {
                Level::Light
            };
            e(self.rec.topology(&prefix, model, &m, level))?;
            self.laws(&prefix, model);
            e(self.rec.fields(&prefix, model))?;
        }
        Ok(())
    }
}

/// The energy names of the records (`steps/energy/<name>`).
pub const ENERGY_NAMES: [&str; 5] = [
    "kinetic",
    "potential",
    "dissipated",
    "reversible",
    "cohesive_contact",
];

/// Run `scenario` on `comm` and compare every recorded quantity with the
/// oracle (bitwise). Global energies: bitwise at 1 and 2 ranks (a sum of
/// two terms has one order); at 4 ranks the port's rank-ordered value must
/// equal the rank-order fold of the oracle's per-rank contributions exactly
/// (D-039 + amendment; upstream reduces with `MPI_Allreduce`), and the
/// port's contributions equal the oracle's bitwise.
pub fn matches_oracle(scenario: &str, comm: &Communicator) -> Outcome {
    let spec = ok(par_scenarios::spec(scenario), "spec")?;
    let fx = fixture(scenario, comm)?;
    let mut obs = OracleObserver::new(&spec, &fx);
    let mesh = distributed(&spec, comm, FacetFlipMode::UpstreamInverse)?;
    let mut bad = Vec::new();
    // the partition (rank 0 records the probe's barycentre rule)
    if comm.rank() == 0 && comm.size() > 1 {
        let mut full = ok(Mesh::new(spec.dim, "full"), "mesh")?;
        ok(par_scenarios::build_mesh(&spec, &mut full, true), "build")?;
        let lin: Vec<i64> = partition(&spec, &full, comm.size())?
            .into_iter()
            .map(i64::from)
            .collect();
        obs.rec
            .q
            .insert("partition/linearized".into(), oracle::Val::I(lin));
    }
    let (mut model, dt, recs) = ok(par_scenarios::run(&spec, mesh.clone(), &mut obs), scenario)?;
    // _smmc_damage, then the damage of every cohesive law
    {
        let distributed_mesh = ok(mesh.read(), "read")?.is_distributed();
        if distributed_mesh {
            ok(
                model.synchronize_cohesive(akantu_core::enums::SynchronizationTag::SmmcDamage),
                "_smmc_damage",
            )?;
            for (m, law) in model.smm.physics.materials.laws().iter().enumerate() {
                let Some(c) = law
                    .as_any()
                    .and_then(|a| a.downcast_ref::<akantu_cohesive::MaterialCohesive>())
                else {
                    continue;
                };
                let field = ok(c.internal("damage"), "damage")?;
                for g in GhostType::ALL {
                    for ty in field.values().element_types(TypeFilter {
                        spatial_dimension: Some(spec.dim),
                        ghost_type: g,
                        kind: Some(ElementKind::Cohesive),
                    }) {
                        let v = field
                            .get(ty, g)
                            .map(|a| a.as_slice().to_vec())
                            .unwrap_or_default();
                        obs.rec.q.insert(
                            format!(
                                "after_smmc_damage/law{m}/damage/{}/{}",
                                ty.name(),
                                if g == GhostType::NotGhost {
                                    "not_ghost"
                                } else {
                                    "ghost"
                                }
                            ),
                            oracle::Val::F(v),
                        );
                    }
                }
            }
        }
    }
    let rec = &mut obs.rec;
    rec.q
        .insert("model/time_step".into(), oracle::Val::F(dt.to_vec()));
    if spec.extrinsic {
        rec.q.insert(
            "steps/inserted".into(),
            oracle::Val::I(recs.iter().map(|r| r.inserted as i64).collect()),
        );
    }
    rec.q.insert(
        "steps/nb_cohesive_local".into(),
        oracle::Val::I(obs.nb_coh.iter().map(|c| c.0).collect()),
    );
    rec.q.insert(
        "steps/nb_cohesive_ghost".into(),
        oracle::Val::I(obs.nb_coh.iter().map(|c| c.1).collect()),
    );
    for (k, name) in ENERGY_NAMES.iter().enumerate() {
        rec.q.insert(
            format!("steps/local_energy/{name}"),
            oracle::Val::F(obs.local_energies.iter().map(|e| e[k]).collect()),
        );
        if comm.size() <= 2 || comm.reduction_policy() == akantu_comm::ReductionPolicy::NativeMpi {
            rec.q.insert(
                format!("steps/energy/{name}"),
                oracle::Val::F(recs.iter().map(|r| r.energies[k]).collect()),
            );
        }
    }
    // no vacuous comparison: every required prefix holds oracle quantities
    let count = |p: &str| {
        fx.manifest()
            .quantities
            .iter()
            .filter(|q| q.name.starts_with(p))
            .count()
    };
    let mut required: Vec<String> = ["model/", "init/", "steps/", "after_smmc_damage/"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    if spec.extrinsic {
        required.push("ins1/".into());
    }
    if comm.rank() == 0 && comm.size() > 1 {
        required.push("partition/".into());
    }
    required.extend(spec.checkpoints.iter().map(|c| format!("s{c}/")));
    for p in &required {
        if count(p) == 0 {
            bad.push(format!("the oracle record has no quantity under {p}"));
        }
    }
    let total = count("");
    if total < 40 {
        bad.push(format!("the oracle record has only {total} quantities"));
    }
    for prefix in [
        "partition/",
        "model/",
        "init/",
        "ins1/",
        "steps/nb_",
        "steps/inserted",
        "steps/local_energy/",
        "after_smmc_damage/",
    ] {
        bad.extend(oracle::compare(&fx, rec, prefix));
    }
    for &cp in &spec.checkpoints {
        bad.extend(oracle::compare(&fx, rec, &format!("s{cp}/")));
    }
    let native = comm.reduction_policy() == akantu_comm::ReductionPolicy::NativeMpi;
    if comm.size() <= 2 || native {
        // one or two terms (one order), or MPI's own reduction as upstream
        bad.extend(oracle::compare(&fx, rec, "steps/energy/"));
    } else {
        // D-039: the port's value is the rank-order fold of the oracle's
        // contributions
        let fxs: Vec<Fixture> = (0..comm.size())
            .map(|r| {
                Fixture::open_by_id(&format!(
                    "parallel_cohesive/{scenario}/np{}/rank{r}",
                    comm.size()
                ))
                .map_err(|e| e.to_string())
            })
            .collect::<std::result::Result<_, _>>()?;
        for (k, name) in ENERGY_NAMES.iter().enumerate() {
            let locals: Vec<Vec<f64>> = fxs
                .iter()
                .map(|f| {
                    f.load_f64(&format!("steps/local_energy/{name}"))
                        .map(|x| x.data)
                        .map_err(|e| e.to_string())
                })
                .collect::<std::result::Result<_, _>>()?;
            for (s, r) in recs.iter().enumerate() {
                let mut fold = locals[0][s];
                for l in &locals[1..] {
                    fold += l[s];
                }
                // kinetic: upstream halves after the reduction
                let have = r.energies[k];
                if !akantu_validation::exact::exact_eq(fold, have) {
                    bad.push(format!(
                        "steps/energy/{name} step {}: port {have:e}, rank-order fold of the \
                         oracle's contributions {fold:e}",
                        s + 1
                    ));
                    break;
                }
            }
        }
    }
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

/// How a check distributes the scenario's mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PartKind {
    /// The probe's barycentre rule (1, 2 and 4 ranks).
    Rule,
    /// Recursive coordinate bisection (any rank count).
    Rcb,
}

/// The scenario's mesh distributed over `comm` (`kind`), facet mesh with
/// the corrected flip (the default of the port).
pub fn distributed_with(
    spec: &Spec,
    comm: &Communicator,
    kind: PartKind,
) -> std::result::Result<SharedMesh, String> {
    if kind == PartKind::Rule {
        return distributed(spec, comm, FacetFlipMode::RemoteOrder);
    }
    let mut m = ok(
        Mesh::with_communicator(spec.dim, "mesh", comm.clone()),
        "mesh",
    )?;
    ok(
        par_scenarios::build_mesh(spec, &mut m, comm.rank() == 0),
        "build",
    )?;
    let mut p = akantu_mesh::partition::RcbPartitioner;
    let _ = ok(
        akantu_mesh::distribution::distribute_with(
            &mut m,
            Some(&mut p as &mut dyn MeshPartitioner),
            0,
        ),
        "distribute",
    )?;
    let _ = ok(
        init_mesh_facets_with(
            &mut m,
            DistributedFacetsOptions {
                flip: FacetFlipMode::RemoteOrder,
            },
        ),
        "init_mesh_facets",
    )?;
    Ok(SharedMesh::new(m))
}

/// Coordinates of the nodes of side 0 of every cohesive element of `ghost`
/// (sorted per element, then the list sorted): the partition-independent
/// identity of the inserted facets (coordinates of doubled nodes are exact
/// copies).
pub fn cohesive_keys(m: &Mesh, ghost: GhostType) -> std::result::Result<Vec<Vec<f64>>, String> {
    let dim = m.spatial_dimension();
    let mut keys = Vec::new();
    for ty in m.element_types(TypeFilter {
        spatial_dimension: Some(dim),
        ghost_type: ghost,
        kind: Some(ElementKind::Cohesive),
    }) {
        let c = ok(m.connectivity(ty, ghost), "conn")?;
        let nn = c.nb_component();
        let x = m.nodes().as_slice().to_vec();
        for row in c.as_slice().chunks_exact(nn) {
            let mut pts: Vec<Vec<f64>> = row[..nn / 2]
                .iter()
                .map(|&n| x[n as usize * dim..(n as usize + 1) * dim].to_vec())
                .collect();
            pts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            keys.push(pts.concat());
        }
    }
    keys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(keys)
}

macro_rules! ensure {
    ($c:expr, $($arg:tt)*) => {
        if !$c {
            return Err(format!($($arg)*));
        }
    };
}

/// Independent checks of a distributed run of `spec` (any rank count with
/// [`PartKind::Rcb`]):
/// * partition independence of the inserted set: the cohesive elements of
///   all ranks (not-ghost, each once) are exactly the serial run's (by the
///   coordinates of their facet);
/// * every ghost cohesive element is in exactly one receive scheme of the
///   cohesive synchronizer, every not-ghost one in no receive scheme;
/// * matched scheme pairs: the send scheme of `p` toward `q` and the
///   receive scheme of `q` from `p` have the same size (cohesive and
///   facet-stress synchronizers);
/// * global ids: the local-or-master nodes of all ranks carry every id of
///   `0..nb_global_nodes` exactly once, every not-pure-ghost node has an id
///   and every id has one set of coordinates on all ranks;
/// * the dissipated energy of the last step is the serial run's within
///   relative 1e-12: an empirical regression bound, not a rounding proof
///   (the partition changes the summation order at every step; measured
///   ≤ 6.7e-14 at 2…8 ranks; NUMERICAL_VALIDATION).
pub fn invariants(spec: &Spec, comm: &Communicator, kind: PartKind) -> Outcome {
    let mesh = distributed_with(spec, comm, kind)?;
    let (model, _dt, recs) = ok(
        par_scenarios::run(spec, mesh.clone(), &mut NoObserverLocal),
        spec.name,
    )?;
    // serial reference on every rank (small meshes)
    let serial_keys;
    let serial_dissipated;
    {
        let mut sm = ok(Mesh::new(spec.dim, "serial"), "mesh")?;
        ok(par_scenarios::build_mesh(spec, &mut sm, true), "build")?;
        let shared = SharedMesh::new(sm);
        let (_m, _dt, srecs) = ok(
            par_scenarios::run(spec, shared.clone(), &mut NoObserverLocal),
            "serial run",
        )?;
        let g = ok(shared.read(), "read")?;
        serial_keys = cohesive_keys(&g, GhostType::NotGhost)?;
        serial_dissipated = srecs.last().map_or(0.0, |r| r.energies[2]);
    }
    let m = ok(mesh.read(), "read")?;
    let dim = spec.dim;
    // 1. inserted set
    let mine = cohesive_keys(&m, GhostType::NotGhost)?;
    // nodes per facet side × dim (one facet type per scenario)
    let width = serial_keys.first().map_or(1, Vec::len);
    let flat: Vec<f64> = mine.concat();
    let all = ok(comm.all_gather_v(&flat), "gather keys")?;
    let mut keys: Vec<Vec<f64>> = all
        .concat()
        .chunks_exact(width)
        .map(<[f64]>::to_vec)
        .collect();
    keys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ensure!(
        keys == serial_keys,
        "{}: {} cohesive elements over the ranks, the serial run has {} (or a different set)",
        spec.name,
        keys.len(),
        serial_keys.len()
    );
    // 2. ghost cohesive elements and the receive schemes
    let par = model
        .ext
        .parallel
        .as_ref()
        .ok_or("no cohesive synchronizers")?;
    let mut seen: std::collections::BTreeMap<akantu_core::Element, usize> = Default::default();
    for scheme in par.cohesive.communications.recv.values() {
        for &e in scheme {
            *seen.entry(e).or_default() += 1;
        }
    }
    for ty in m.element_types(TypeFilter {
        spatial_dimension: Some(dim),
        ghost_type: GhostType::Ghost,
        kind: Some(ElementKind::Cohesive),
    }) {
        for i in 0..m.nb_element(ty, GhostType::Ghost) {
            let e = ok(akantu_core::Element::try_new(ty, i, GhostType::Ghost), "el")?;
            ensure!(
                seen.get(&e) == Some(&1),
                "ghost cohesive element {e} is in {} receive scheme(s)",
                seen.get(&e).copied().unwrap_or(0)
            );
        }
    }
    ensure!(
        seen.keys().all(|e| e.ghost_type() == GhostType::Ghost),
        "a not-ghost element is in a receive scheme of the cohesive synchronizer"
    );
    // 2b. every ghost cohesive element has its owner's global connectivity
    //     (nodes that are pure ghosts here carry no id and are skipped)
    {
        let mut acc = GlobalConnCheck {
            mesh: &m,
            bad: Vec::new(),
        };
        ok(
            par.cohesive
                .synchronize(&mut acc, akantu_core::enums::SynchronizationTag::Test),
            "global connectivity exchange",
        )?;
        ensure!(
            acc.bad.is_empty(),
            "ghost cohesive elements differ from their owner's global connectivity: {:?}",
            &acc.bad[..acc.bad.len().min(5)]
        );
    }
    // 3. matched scheme pairs
    let np = comm.size();
    for (name, sync) in [
        ("cohesive", Some(&par.cohesive)),
        ("facet stress", par.facet_stress.as_ref()),
    ] {
        let Some(sync) = sync else { continue };
        let mut sizes = vec![0u64; 2 * np];
        for (&q, s) in &sync.communications.send {
            sizes[q as usize] = s.len() as u64;
        }
        for (&q, s) in &sync.communications.recv {
            sizes[np + q as usize] = s.len() as u64;
        }
        let all = ok(comm.all_gather(&sizes), "gather sizes")?;
        for p in 0..np {
            for q in 0..np {
                let send_pq = all[p * 2 * np + q];
                let recv_qp = all[q * 2 * np + np + p];
                ensure!(
                    send_pq == recv_qp,
                    "{name} synchronizer: rank {p} sends {send_pq} entries to {q}, which receives {recv_qp}"
                );
            }
        }
    }
    // 4. global ids
    let n = m.nb_nodes();
    let mut owned = Vec::new();
    let mut ids_coords: Vec<f64> = Vec::new();
    for i in 0..n {
        let f = m.node_flag(i as Idx);
        let g = m.node_global_id(i as Idx);
        if f.is_pure_ghost() {
            continue;
        }
        ensure!(g >= 0, "node {i} (flag {:#x}) has no global id", f.bits());
        if f.is_local_or_master() {
            owned.push(g);
        }
        ids_coords.push(g as f64);
        ids_coords.extend_from_slice(&m.nodes().as_slice()[i * dim..(i + 1) * dim]);
    }
    let all_owned: Vec<i64> = ok(comm.all_gather_v(&owned), "gather ids")?.concat();
    let mut sorted = all_owned.clone();
    sorted.sort_unstable();
    let ng = m.nb_global_nodes() as i64;
    ensure!(
        sorted == (0..ng).collect::<Vec<_>>(),
        "owned global ids are not 0..{ng} exactly once ({} owned entries)",
        sorted.len()
    );
    let all_ic: Vec<f64> = ok(comm.all_gather_v(&ids_coords), "gather coords")?.concat();
    let mut by_id: std::collections::BTreeMap<i64, Vec<f64>> = Default::default();
    for row in all_ic.chunks_exact(dim + 1) {
        let id = row[0] as i64;
        let x = row[1..].to_vec();
        if let Some(prev) = by_id.get(&id) {
            ensure!(
                *prev == x,
                "global id {id} has two positions: {prev:?} and {x:?}"
            );
        } else {
            by_id.insert(id, x);
        }
    }
    // 5. dissipated energy vs serial
    let d = recs.last().map_or(0.0, |r| r.energies[2]);
    let rel = (d - serial_dissipated).abs() / serial_dissipated.abs().max(f64::MIN_POSITIVE);
    // NaN-propagating: a NaN energy fails
    let close = rel <= 1e-12;
    ensure!(
        close,
        "{}: dissipated energy {d:e} vs serial {serial_dissipated:e} (rel {rel:e})",
        spec.name
    );
    Ok(())
}

/// No hooks (local alias of the library's observer).
pub struct NoObserverLocal;
impl RunObserver for NoObserverLocal {}

/// Owners send the global ids of their cohesive elements' nodes; the ghost
/// copies compare them with their own (pure-ghost nodes skipped).
struct GlobalConnCheck<'a> {
    mesh: &'a Mesh,
    bad: Vec<String>,
}

impl akantu_comm::DataAccessor<akantu_core::Element> for GlobalConnCheck<'_> {
    fn nb_data(
        &self,
        els: &[akantu_core::Element],
        _: akantu_core::enums::SynchronizationTag,
    ) -> usize {
        els.iter().map(|e| e.ty().nb_nodes_per_element() * 8).sum()
    }
    fn pack_data(
        &self,
        b: &mut akantu_comm::buffer::CommunicationBuffer,
        els: &[akantu_core::Element],
        _: akantu_core::enums::SynchronizationTag,
    ) -> akantu_core::Result<()> {
        for &e in els {
            for &n in &self.mesh.element_connectivity(e)? {
                b.pack(&self.mesh.node_global_id(n));
            }
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut akantu_comm::buffer::CommunicationBuffer,
        els: &[akantu_core::Element],
        _: akantu_core::enums::SynchronizationTag,
    ) -> akantu_core::Result<()> {
        for &e in els {
            for &n in &self.mesh.element_connectivity(e)? {
                let g: i64 = b.unpack()?;
                if !self.mesh.node_flag(n).is_pure_ghost() && self.mesh.node_global_id(n) != g {
                    self.bad.push(format!(
                        "{e} node {n}: {} vs owner {g}",
                        self.mesh.node_global_id(n)
                    ));
                }
            }
        }
        Ok(())
    }
}
