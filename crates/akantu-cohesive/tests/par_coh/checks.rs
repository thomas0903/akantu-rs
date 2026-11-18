// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). test
// support (no upstream counterpart): the collective error protocol (D-018,
// D-126) on the distributed extrinsic cohesive scenario of
// oracle/probes/parallel_cohesive, one rank's part, shared by the
// local-world tests (collective_checks_failures.rs,
// collective_checks_state.rs) and the real-MPI binary (mpi_cohesive.rs).

use std::time::{Duration, Instant};

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::par_scenarios::{self, NoObserver, RunObserver};
use akantu_comm::Communicator;
use akantu_comm::phase::{CollectiveChecks, testing};
use akantu_core::{FatalKind, Real, Result};
use akantu_mesh::SharedMesh;
use akantu_model::constitutive_law::InternalFieldRef;

/// Both collective-check modes.
pub const MODES: [CollectiveChecks; 2] =
    [CollectiveChecks::EveryPhase, CollectiveChecks::CommitPoints];

fn distributed(comm: &Communicator, spec: &par_scenarios::Spec) -> SharedMesh {
    super::distributed(
        spec,
        comm,
        akantu_mesh::utils::FacetFlipMode::UpstreamInverse,
    )
    .expect("distributed mesh")
}

/// Arms the injection after the initialisation, so that only the steps
/// count.
struct Arm {
    pattern: String,
    nth: u32,
    fail: bool,
    mesh: SharedMesh,
    start: u64,
}

impl RunObserver for Arm {
    fn after_init(&mut self, _m: &mut SolidMechanicsModelCohesive) -> Result<()> {
        // revisions committed from here on are collective (identical
        // counts on every rank when no rank commits alone)
        self.start = self.mesh.revision()?;
        if self.fail {
            testing::fail_phase(&self.pattern, self.nth);
        }
        Ok(())
    }
}

/// What one rank observed in a run with an injected failure.
#[derive(Debug)]
pub struct Failure {
    /// The run's error (`None`: it succeeded).
    pub error: Option<String>,
    /// Its fatal kind.
    pub fatal: Option<FatalKind>,
    /// Mesh revisions committed during the steps.
    pub revision: u64,
    /// Duration of the run.
    pub elapsed: Duration,
}

/// The scenario's run on this rank with an injected failure of `pattern`
/// (its `nth` occurrence) on rank `failing`.
pub fn failure_run(
    comm: &Communicator,
    mode: CollectiveChecks,
    pattern: &str,
    nth: u32,
    failing: usize,
) -> Failure {
    let spec = par_scenarios::spec("extrinsic_tri").unwrap();
    comm.set_collective_checks(mode);
    let mesh = distributed(comm, &spec);
    let mut arm = Arm {
        pattern: pattern.to_owned(),
        nth,
        fail: usize::try_from(comm.rank()).unwrap() == failing,
        mesh: mesh.clone(),
        start: 0,
    };
    let t0 = Instant::now();
    let r = par_scenarios::run(&spec, mesh.clone(), &mut arm);
    let elapsed = t0.elapsed();
    testing::clear();
    let revision = mesh.revision().map_or(u64::MAX, |r| r - arm.start);
    let (error, fatal) = match r {
        Ok(_) => (None, None),
        Err(e) => (Some(e.to_string()), e.fatal_kind()),
    };
    Failure {
        error,
        fatal,
        revision,
        elapsed,
    }
}

/// This rank's part of the check of a [`failure_run`]: an error (the
/// injected one on the failing rank, a remote failure elsewhere), the
/// expected fatal kind, no wait as long as `bound`.
///
/// # Errors
/// What differs.
pub fn check_failure(
    s: &Failure,
    rank: usize,
    failing: usize,
    fatal: Option<FatalKind>,
    bound: Duration,
    what: &str,
) -> std::result::Result<(), String> {
    let e = s
        .error
        .as_deref()
        .ok_or_else(|| format!("{what}: rank {rank} ended without an error"))?;
    let expected = if rank == failing {
        e.contains("injected failure")
    } else {
        e.contains("failed on another rank") || e.contains("injected failure")
    };
    if !expected {
        return Err(format!("{what}: rank {rank}: {e}"));
    }
    if s.fatal != fatal {
        return Err(format!(
            "{what}: rank {rank}: fatal kind {:?}, expected {fatal:?}: {e}",
            s.fatal
        ));
    }
    if s.elapsed >= bound {
        return Err(format!(
            "{what}: rank {rank} took {:?} (a wait ran into the timeout)",
            s.elapsed
        ));
    }
    Ok(())
}

/// The failure scenarios: the pattern of the injected phase, its
/// occurrence and the expected agreed fatal kind.
pub const FAILURES: [(&str, &str, u32, Option<FatalKind>); 5] = [
    // a model callback of the explicit solve (the residual assembly)
    ("solve_step", "model:assemble_residual", 3, None),
    // the insertion check's first decision (are facets flagged anywhere)
    ("insertion_check", "ce:insertion_any:prepare", 3, None),
    // the energies' reduction
    ("energy", "smm:energy/smm:all_reduce:prepare", 2, None),
    // the first insertion's mesh transaction before its commit: every rank
    // rolls back
    (
        "transaction_before_commit",
        "mesh-mutation/topology",
        1,
        None,
    ),
    // a consumer of the first insertion's transaction after the commit: the
    // domain is poisoned on every rank
    (
        "transaction_after_commit",
        "mesh-mutation/dispatch",
        1,
        Some(FatalKind::Agreed),
    ),
];

// ---------------------------------------------------------------------------
// Persistent state after a failure of the explicit solve step
// ---------------------------------------------------------------------------

/// Named bit patterns of one rank's persistent state.
pub type State = Vec<(String, Vec<u64>)>;

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

/// The DOF arrays, the lumped mass, the DOF manager's blocked snapshot
/// (indexes, flags, release and whether it changed since the previous
/// one), every `Real` internal of every law (current and previous values)
/// and the mesh's revision and sizes.
fn state(model: &SolidMechanicsModelCohesive, mesh: &SharedMesh) -> State {
    let mut out = State::new();
    let smm = &model.smm;
    out.push((
        "displacement".into(),
        bits(smm.displacement().unwrap().as_slice()),
    ));
    out.push(("velocity".into(), bits(smm.velocity().unwrap().as_slice())));
    out.push((
        "acceleration".into(),
        bits(smm.acceleration().unwrap().as_slice()),
    ));
    if let Ok(p) = smm.previous_displacement() {
        out.push(("previous displacement".into(), bits(p.as_slice())));
    }
    if let Ok(i) = smm.increment() {
        out.push(("increment".into(), bits(i.as_slice())));
    }
    if let Ok(m) = smm.mass() {
        out.push(("lumped mass".into(), bits(m.as_slice())));
    }
    // the DOF manager's blocked snapshot and its releases
    let dm = smm.dof_manager();
    out.push((
        "blocked snapshot".into(),
        dm.global_blocked_dofs_indexes()
            .iter()
            .map(|&i| i as u64)
            .chain(dm.global_blocked_dofs().iter().map(|&b| u64::from(b)))
            .chain([
                dm.blocked_release(),
                u64::from(dm.has_blocked_dofs_changed()),
            ])
            .collect(),
    ));
    for l in 0..smm.nb_materials() {
        let law = smm.material(l).unwrap();
        law.for_each_internal(&mut |name, f| {
            let InternalFieldRef::Real(f) = f else {
                return;
            };
            let values = f.values().to_plain().unwrap();
            for (ty, ghost, a) in values.as_map().iter_all() {
                let key = format!("law {l}/{name}/{ty:?}/{ghost:?}");
                out.push((key.clone(), bits(a.as_slice())));
                if f.has_history() {
                    out.push((
                        format!("{key}/previous"),
                        bits(f.previous(ty, ghost).unwrap().as_slice()),
                    ));
                }
            }
        });
    }
    let m = mesh.read().unwrap();
    out.push((
        "mesh".into(),
        vec![
            mesh.revision().unwrap(),
            m.nb_nodes() as u64,
            m.nodes().as_slice().len() as u64,
        ],
    ));
    let mut sizes = Vec::new();
    for ghost in akantu_core::GhostType::ALL {
        for ty in m.element_types(akantu_core::TypeFilter {
            ghost_type: ghost,
            ..Default::default()
        }) {
            sizes.push(m.nb_element(ty, ghost) as u64);
        }
    }
    out.push(("mesh elements".into(), sizes));
    out
}

/// What one rank saw at the solve step with the injection.
#[derive(Debug)]
pub struct StepState {
    /// The solve step's error (`None`: it succeeded).
    pub error: Option<String>,
    /// The persistent state after the solve step.
    pub state: State,
    /// The phase keys of the solve step (recording runs only).
    pub keys: Vec<String>,
}

/// The extrinsic scenario's steps up to `offset` steps after the first
/// insertion (0: the step that inserts, whose solve reassembles the mass;
/// 2: cohesive elements exist and open), the boundary conditions changed
/// before the last one's solve (y also blocked on the pulled faces), that
/// solve with
/// `inject` (pattern, occurrence) armed on rank `failing`, or recording its
/// phase keys on every rank when `inject` is `None`.
pub fn state_run(
    comm: &Communicator,
    offset: usize,
    mode: CollectiveChecks,
    inject: Option<(&str, u32)>,
    failing: usize,
) -> StepState {
    let spec = par_scenarios::spec("extrinsic_tri").unwrap();
    let nb_steps = spec.nb_steps;
    // the scenario's initialisation only; the steps run here
    let init = par_scenarios::Spec {
        nb_steps: 0,
        ..spec.clone()
    };
    comm.set_collective_checks(mode);
    let mesh = distributed(comm, &spec);
    let (mut model, [_, dt], _) =
        par_scenarios::run(&init, mesh.clone(), &mut NoObserver).expect("initialisation");
    let dim = spec.dim;
    let mut last = None;
    for step in 1..=nb_steps {
        // the scenario's loading (pull in x), as `par_scenarios::run`
        let coords = mesh.read().unwrap().nodes().as_slice().to_vec();
        {
            let mut disp = model.smm.displacement_mut().unwrap();
            let u = disp.as_mut_slice();
            for n in 0..coords.len() / dim {
                let xx = par_scenarios::loading_coordinate(&spec, coords[dim * n]);
                if xx.abs() > 0.99 {
                    u[dim * n] += spec.rate * dt * xx;
                }
            }
        }
        model.check_cohesive_stress().expect("insertion check");
        if last.is_none() && model.last_insertion_global_count().unwrap_or(0) > 0 {
            last = Some(step + offset);
        }
        if last == Some(step) {
            // the boundary conditions change before this solve (y also
            // blocked on the pulled faces), so that the solve's update of
            // the blocked snapshot changes it
            {
                let mut b_g = model.smm.blocked_dofs_mut().unwrap();
                let b = b_g.as_mut_slice();
                for n in 0..coords.len() / dim {
                    let xx = par_scenarios::loading_coordinate(&spec, coords[dim * n]);
                    if xx.abs() > 0.99 {
                        b[dim * n + 1] = true;
                    }
                }
            }
            let me = usize::try_from(comm.rank()).unwrap() == failing;
            match inject {
                Some((p, n)) if me => testing::fail_phase(p, n),
                Some(_) => {}
                None => testing::record_keys(),
            }
            let r = model.solve_step(None);
            let keys = testing::recorded_keys();
            testing::clear();
            return StepState {
                error: r.err().map(|e| e.to_string()),
                state: state(&model, &mesh),
                keys,
            };
        }
        model.solve_step(None).expect("solve step");
    }
    panic!("no insertion in {nb_steps} steps");
}

/// The injection of the phase `every[i]` in each mode: the key and its
/// occurrence number among the keys of that mode's run that contain it (the
/// injection matches by substring, and a commit point's key may contain a
/// phase's key).
pub fn injection_points(every: &[String], commit: &[String]) -> Vec<(String, u32, u32)> {
    let nth = |keys: &[String], k: &str, exact: usize| -> u32 {
        let j = keys
            .iter()
            .enumerate()
            .filter(|(_, o)| o.as_str() == k)
            .nth(exact)
            .map(|(j, _)| j)
            .unwrap_or_else(|| panic!("phase {k} missing in a run"));
        u32::try_from(keys[..=j].iter().filter(|o| o.contains(k)).count()).unwrap()
    };
    every
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let exact = every[..i].iter().filter(|o| *o == k).count();
            (k.clone(), nth(every, k, exact), nth(commit, k, exact))
        })
        .collect()
}

/// The phases of the commit-points run without its commit points'
/// agreements of the deferred outcomes (no phase under every-phase checks).
pub fn without_commit_points(keys: &[String]) -> Vec<&String> {
    keys.iter().filter(|k| !k.ends_with(":deferred")).collect()
}

/// Bitwise comparison of two states.
///
/// # Errors
/// The first difference.
pub fn compare(a: &State, b: &State, what: &str) -> std::result::Result<(), String> {
    let ka: Vec<&String> = a.iter().map(|(k, _)| k).collect();
    let kb: Vec<&String> = b.iter().map(|(k, _)| k).collect();
    if ka != kb {
        return Err(format!("{what}: different state layouts"));
    }
    for ((k, x), (_, y)) in a.iter().zip(b) {
        if x != y {
            return Err(format!("{what}: '{k}' differs"));
        }
    }
    Ok(())
}

/// One [`state_run`] on every rank: the mode, the injection (pattern,
/// occurrence; `None` records the phase keys) and the failing rank.
pub type StateRun<'a> = dyn FnMut(CollectiveChecks, Option<(&str, u32)>, usize) -> StepState + 'a;

/// This rank's part of the state check: a run without failure in each
/// mode (the same phases, the same state), then for every phase of the
/// solve step of rank `failing` (each `failing` of `failing_ranks`) a run
/// with that phase failing on it in each mode: every rank fails, and its
/// persistent state is the same in both modes. `run` runs
/// [`state_run`] on every rank (the local world or MPI). Returns the number
/// of injected failures compared.
///
/// # Errors
/// What differs on this rank.
pub fn check_states(
    rank: usize,
    failing_ranks: &[usize],
    run: &mut StateRun<'_>,
    what: &str,
) -> std::result::Result<usize, String> {
    // every rank runs every injection whatever it finds (a rank that left
    // the loop early would leave its peers waiting in the next run); the
    // differences are collected and reported at the end
    let mut errors = Vec::new();
    let reference = run(CollectiveChecks::EveryPhase, None, usize::MAX);
    let control = run(CollectiveChecks::CommitPoints, None, usize::MAX);
    for s in [&reference, &control] {
        if let Some(e) = &s.error {
            errors.push(format!("{what}: no failure injected, rank {rank}: {e}"));
        }
    }
    if reference.keys.iter().collect::<Vec<_>>() != without_commit_points(&control.keys) {
        errors.push(format!(
            "{what}: rank {rank} runs other phases in the two modes"
        ));
    }
    if let Err(e) = compare(
        &reference.state,
        &control.state,
        &format!("{what}, no failure, rank {rank}"),
    ) {
        errors.push(e);
    }
    let points = injection_points(&reference.keys, &control.keys);
    if points.len() <= 10 {
        errors.push(format!("{what}: {} phases recorded", points.len()));
    }
    let mut n = 0;
    for &failing in failing_ranks {
        for (pattern, nth_every, nth_commit) in &points {
            let w = format!("{what}: {pattern} #{nth_every}, rank {failing} failing, rank {rank}");
            let every = run(
                CollectiveChecks::EveryPhase,
                Some((pattern, *nth_every)),
                failing,
            );
            let commit = run(
                CollectiveChecks::CommitPoints,
                Some((pattern, *nth_commit)),
                failing,
            );
            for (m, s) in [("every-phase", &every), ("commit points", &commit)] {
                match s.error.as_deref() {
                    None => errors.push(format!("{w}: {m}: ended without an error")),
                    Some(e) if rank == failing && !e.contains("injected failure") => {
                        errors.push(format!("{w}: {m}: {e}"));
                    }
                    Some(_) => {}
                }
            }
            if let Err(e) = compare(&every.state, &commit.state, &w) {
                errors.push(e);
            }
            n += 1;
        }
    }
    if errors.is_empty() {
        Ok(n)
    } else {
        Err(errors.join("\n"))
    }
}
