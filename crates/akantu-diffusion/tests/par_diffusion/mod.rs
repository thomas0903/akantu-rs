// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Scenarios
// (no runnable upstream counterpart: upstream's distributed DiffusionModel
// fails at its first residual assembly, UD-275): distributed vs serial
// HeatTransferModel runs, the model's synchronisation tags, the UD-275
// compat switch and agreed rank-specific initialisation failures. Shared by
// the local-world tests (`diffusion_parallel.rs`) and the real-MPI binary
// (`mpi_diffusion.rs`).

//! Diffusion on distributed meshes.
//!
//! The mesh is a structured cube of `n³` hexahedra split into 6 tetrahedra
//! each (as the SMM scenarios), built on rank 0 and distributed with the
//! pure-Rust bisection partitioner; the same mesh is solved serially on
//! rank 0 and the results are compared per **global node** on rank 0.

#![allow(dead_code)]

use akantu_comm::{Communicator, Synchronizer};
use akantu_core::enums::{AnalysisMethod, SparseSolverType, SynchronizationTag};
use akantu_core::{ElementType, GhostType, Idx, Real, Result};
use akantu_diffusion::{DiffusionUpstreamCompat, HeatTransferModel};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::synchronizers::NodeEntities;
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;

pub type Outcome = std::result::Result<(), String>;

pub fn ok<T>(r: Result<T>, what: &str) -> std::result::Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

/// Anisotropic, temperature-dependent conductivity (the static and
/// implicit solves are non-linear).
pub const DAT: &str = "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    \
    name = mat\n    density = 2\n    capacity = 3\n    \
    conductivity = [[2.0, 0.3, 0.1], [0.2, 1.5, 0.2], [0.1, 0.4, 1.0]]\n    \
    conductivity_variation = 0.01\n    temperature_reference = 10\n  ]\n]\n";

/// Structured cube `[0, 1]³` of `n³` hexahedra, each split into 6
/// tetrahedra (Kuhn triangulation).
pub fn cube_tet4(n: usize) -> (Vec<[Real; 3]>, Vec<[Idx; 4]>) {
    let h = 1.0 / n as Real;
    let id = |i: usize, j: usize, k: usize| ((k * (n + 1) + j) * (n + 1) + i) as Idx;
    let mut nodes = Vec::new();
    for k in 0..=n {
        for j in 0..=n {
            for i in 0..=n {
                nodes.push([i as Real * h, j as Real * h, k as Real * h]);
            }
        }
    }
    let mut tets = Vec::new();
    for k in 0..n {
        for j in 0..n {
            for i in 0..n {
                let v = [
                    id(i, j, k),
                    id(i + 1, j, k),
                    id(i + 1, j + 1, k),
                    id(i, j + 1, k),
                    id(i, j, k + 1),
                    id(i + 1, j, k + 1),
                    id(i + 1, j + 1, k + 1),
                    id(i, j + 1, k + 1),
                ];
                for t in [
                    [0, 1, 2, 6],
                    [0, 2, 3, 6],
                    [0, 3, 7, 6],
                    [0, 7, 4, 6],
                    [0, 4, 5, 6],
                    [0, 5, 1, 6],
                ] {
                    tets.push([v[t[0]], v[t[1]], v[t[2]], v[t[3]]]);
                }
            }
        }
    }
    (nodes, tets)
}

/// The cube mesh on `comm` (full on rank 0, empty elsewhere).
pub fn cube_mesh(comm: Communicator, n: usize, full: bool) -> std::result::Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(3, "cube", comm), "mesh")?;
    if full {
        let (nodes, tets) = cube_tet4(n);
        let mut a = MeshAccessor::new(&mut m);
        for x in &nodes {
            ok(a.push_node(x), "node")?;
        }
        let mut c = a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
        for t in &tets {
            c.push_tuple(t);
        }
    }
    ok(m.compute_bounding_box(), "bbox")?;
    Ok(m)
}

/// Distributed cube (`comm.size()` parts, bisection partitioner).
pub fn distributed_cube(comm: &Communicator, n: usize) -> std::result::Result<Mesh, String> {
    let mut m = cube_mesh(comm.clone(), n, comm.rank() == 0)?;
    let mut p = RcbPartitioner;
    let _ = ok(
        distribute_with(&mut m, Some(&mut p as &mut dyn MeshPartitioner), 0),
        "distribute",
    )?;
    Ok(m)
}

/// Kind of run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Run {
    /// Forward Euler, lumped capacity.
    Explicit,
    /// Non-linear static (Newton–Raphson), faer (centralised).
    Static,
    /// Backward Euler (Newton–Raphson), faer.
    Implicit,
}

/// Result of a run, gathered per global node on rank 0.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub t: Vec<Real>,
    pub rate: Vec<Real>,
    pub thermal: Real,
    pub stable_dt: Real,
}

/// Gather a nodal array (one component) per global node on rank 0.
fn gather(
    mesh: &Mesh,
    a: &[Real],
    parallel: bool,
) -> std::result::Result<Option<Vec<Real>>, String> {
    if parallel {
        let ns: &Synchronizer<Idx> = mesh.synchronizers().node.as_ref().ok_or("no node sync")?;
        let e = NodeEntities { mesh };
        ok(ns.gather(&e, a, 1), "gather")
    } else if mesh.communicator().rank() == 0 {
        let mut g = vec![0.0; a.len()];
        for (n, an) in a.iter().enumerate().take(mesh.nb_nodes()) {
            let gid = usize::try_from(mesh.node_global_id(n as Idx)).unwrap_or(0);
            g[gid] = *an;
        }
        Ok(Some(g))
    } else {
        Ok(None)
    }
}

/// Set up and run a heat-transfer model on `mesh`; returns the gathered
/// snapshot on rank 0.
pub fn run(mesh: Mesh, kind: Run, steps: usize) -> std::result::Result<Option<Snapshot>, String> {
    let comm = mesh.communicator().clone();
    let parallel = mesh.synchronizers().node.is_some() && comm.size() > 1;
    let parser = ok(Parser::parse_str(DAT, "heat.dat"), "parser")?;
    let shared = SharedMesh::new(mesh);
    let mut model = ok(
        HeatTransferModel::with_parser(shared.clone(), None, "heat_transfer", parser),
        "model",
    )?;
    let method = match kind {
        Run::Explicit => AnalysisMethod::ExplicitLumpedMass,
        Run::Static => AnalysisMethod::Static,
        Run::Implicit => AnalysisMethod::ImplicitDynamic,
    };
    let mut options = ModelOptions::new(method);
    if kind != Run::Explicit {
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    ok(model.init_full(&options), "init_full")?;
    let (coords, nb) = {
        let m = ok(shared.read(), "read")?;
        (m.nodes().as_slice().to_vec(), m.nb_nodes())
    };
    // z = 0 face at 100, x = 1 face at 0; interior: a smooth field
    {
        let mut t_g = ok(model.temperature_mut(), "T")?;
        let t = t_g.as_mut_slice();
        for n in 0..nb {
            let (x, y, z) = (coords[3 * n], coords[3 * n + 1], coords[3 * n + 2]);
            t[n] = 20. + 30. * x * (1. - y) + 10. * z * z;
        }
    }
    for n in 0..nb {
        let (x, z) = (coords[3 * n], coords[3 * n + 2]);
        if z == 0.0 {
            ok(model.blocked_dofs_mut(), "blocked")?.as_mut_slice()[n] = true;
            ok(model.temperature_mut(), "T")?.as_mut_slice()[n] = 100.;
        } else if x == 1.0 {
            ok(model.blocked_dofs_mut(), "blocked")?.as_mut_slice()[n] = true;
            ok(model.temperature_mut(), "T")?.as_mut_slice()[n] = 0.;
        }
    }
    let stable_dt = ok(model.stable_time_step(), "stable time step")?;
    match kind {
        Run::Explicit => ok(model.set_time_step(0.5 * stable_dt, None), "dt")?,
        Run::Implicit => ok(model.set_time_step(20. * stable_dt, None), "dt")?,
        Run::Static => {}
    }
    if kind != Run::Explicit {
        let nls = ok(model.solver.non_linear_solver_mut(None), "nls")?;
        use akantu_parser::param::ParamValue;
        ok(
            nls.set_param("max_iterations", ParamValue::Int(30)),
            "max_it",
        )?;
        ok(
            nls.set_param("threshold", ParamValue::Real(1e-10)),
            "threshold",
        )?;
    }
    let steps = if kind == Run::Static { 1 } else { steps };
    for s in 0..steps {
        ok(model.solve_step(None), &format!("solve step {s}"))?;
    }
    let thermal = ok(model.energy("thermal"), "thermal energy")?;
    let t = ok(model.temperature(), "T")?.as_slice().to_vec();
    let rate = if kind == Run::Static {
        vec![0.0; t.len()]
    } else {
        ok(model.temperature_rate(), "rate")?.as_slice().to_vec()
    };
    let m = ok(shared.read(), "read")?;
    let gt = gather(&m, &t, parallel)?;
    let gr = gather(&m, &rate, parallel)?;
    Ok(match (gt, gr) {
        (Some(t), Some(rate)) => Some(Snapshot {
            t,
            rate,
            thermal,
            stable_dt,
        }),
        _ => None,
    })
}

/// Largest absolute difference, largest magnitude, bitwise-equal count.
pub fn diff(a: &[Real], b: &[Real]) -> (Real, Real, usize) {
    let mut d = 0.0;
    let mut mag = 0.0;
    let mut bitwise = 0;
    for (x, y) in a.iter().zip(b) {
        d = Real::max(d, (x - y).abs());
        mag = Real::max(mag, x.abs());
        if x.to_bits() == y.to_bits() {
            bitwise += 1;
        }
    }
    (d, mag, bitwise)
}

/// Distributed vs serial run of `kind`: per global node the temperature
/// and its rate differ by at most `rel_tol` relative to the largest value
/// (summation order at interface nodes: rank-partial internal flows and
/// lumped capacities are reduced on their owners; the centralised faer
/// solve sees the same matrix in another assembly order), the thermal
/// energy likewise, the stable time step exactly (a minimum is
/// order-independent); one rank exactly for explicit and static runs.
/// Tolerances (PROPOSED, NUMERICAL_VALIDATION
/// Parallel): explicit 1e-13, static 1e-14, implicit 1e-12 — ≥ 50× the
/// largest difference measured at 1/2/4 ranks on aarch64 and x86_64.
pub fn distributed_vs_serial(
    comm: Communicator,
    kind: Run,
    n: usize,
    steps: usize,
    rel_tol: Real,
) -> Outcome {
    let rank = comm.rank();
    let size = comm.size();
    let m = distributed_cube(&comm, n)?;
    let par = run(m, kind, steps)?;
    if rank != 0 {
        return Ok(());
    }
    let par = par.ok_or("no gathered result")?;
    let ser =
        run(cube_mesh(Communicator::serial(), n, true)?, kind, steps)?.ok_or("no serial result")?;
    let (dt_, mt, et) = diff(&par.t, &ser.t);
    let (dr, mr, er) = diff(&par.rate, &ser.rate);
    println!(
        "INFO diffusion {kind:?} n={n} ranks={size}: |dT|max={dt_:.3e} (|T|max={mt:.3e}, \
         {et}/{} bitwise), |dTdot|max={dr:.3e} (|Tdot|max={mr:.3e}, {er}/{} bitwise), \
         E {:.17e} vs {:.17e}, dt {:.17e} vs {:.17e}",
        par.t.len(),
        par.rate.len(),
        par.thermal,
        ser.thermal,
        par.stable_dt,
        ser.stable_dt
    );
    if par.stable_dt.to_bits() != ser.stable_dt.to_bits() {
        return Err(format!(
            "stable time step {} vs {}",
            par.stable_dt, ser.stable_dt
        ));
    }
    if dt_ > rel_tol * mt.max(Real::MIN_POSITIVE) || dr > rel_tol * mr.max(Real::MIN_POSITIVE) {
        return Err(format!(
            "{kind:?} at {size} ranks differs from serial: dT {dt_:e} (|T| {mt:e}), dTdot {dr:e} (|Tdot| {mr:e})"
        ));
    }
    if (par.thermal - ser.thermal).abs() > rel_tol * ser.thermal.abs() {
        return Err(format!("thermal energy {} vs {}", par.thermal, ser.thermal));
    }
    // one rank: the distributed mesh is the serial one (no ghost, same
    // numbering, same reduction order, D-039): explicit and static runs are
    // bitwise identical (the implicit one multiplies by `M` through the
    // distributed product, UD-003 corrected, as SMM)
    if size == 1 && kind != Run::Implicit && (dt_ != 0.0 || dr != 0.0) {
        return Err("one rank must equal the serial run bitwise".into());
    }
    Ok(())
}

fn model_on(
    comm: &Communicator,
    n: usize,
) -> std::result::Result<(HeatTransferModel, SharedMesh), String> {
    let m = distributed_cube(comm, n)?;
    let parser = ok(Parser::parse_str(DAT, "heat.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let model = ok(
        HeatTransferModel::with_parser(shared.clone(), None, "heat_transfer", parser),
        "model",
    )?;
    Ok((model, shared))
}

/// The model's registered tags (DIFF-010): `_diffusion` over the element
/// synchronizer gives every node of a ghost element its owner's value;
/// `_diffusion_gradient` exchanges `∇u` (ghost gradients equal the ones the
/// ghost computes itself from synchronised nodal values); the DOF tag
/// `_diffusion` restores slave values from their masters; ghost elements
/// carry their owner's law; unregistered tags are refused.
pub fn model_synchronize_tags(comm: Communicator) -> Outcome {
    use SynchronizationTag as T;
    let (mut model, shared) = model_on(&comm, 3)?;
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init",
    )?;
    let (flags, gids, ghost_nodes): (Vec<akantu_core::NodeFlag>, Vec<i64>, Vec<usize>) = {
        let mesh = ok(shared.read(), "read")?;
        let mut gn = std::collections::BTreeSet::new();
        for e in mesh.elements(
            akantu_core::TypeFilter::new()
                .dim(3)
                .ghost(GhostType::Ghost),
        ) {
            ok(model.physics.laws.law_of(e), &format!("law of ghost {e}"))?;
            for &nd in &ok(mesh.element_connectivity(e), "conn")? {
                gn.insert(nd as usize);
            }
        }
        (
            mesh.nodes_flags().as_slice().to_vec(),
            (0..mesh.nb_nodes())
                .map(|n| mesh.node_global_id(n as Idx))
                .collect(),
            gn.into_iter().collect(),
        )
    };
    let value = |g: i64| 0.5 * g as Real + 1.0;
    // garbage on the nodes whose flag satisfies `bad`, the global value
    // elsewhere (the element exchange writes the owner rank's values of
    // every node of its elements, so shared nodes must hold consistent
    // values, as they do after a solve)
    let set_garbage =
        |model: &mut HeatTransferModel, bad: &dyn Fn(akantu_core::NodeFlag) -> bool| -> Outcome {
            let mut t_g = ok(model.temperature_mut(), "T")?;
            let t = t_g.as_mut_slice();
            for (n, f) in flags.iter().enumerate() {
                t[n] = if bad(*f) { -7.0 } else { value(gids[n]) };
            }
            Ok(())
        };
    // element synchronizer: pure-ghost nodes (no DOF equation, never
    // updated by the solver) restored from the owners
    set_garbage(&mut model, &|f| f.is_pure_ghost())?;
    ok(model.synchronize(T::Diffusion), "synchronize _diffusion")?;
    {
        let t_g = ok(model.temperature(), "T")?;
        let t = t_g.as_slice();
        for &n in &ghost_nodes {
            if t[n] != value(gids[n]) {
                return Err(format!(
                    "node {n} of a ghost element (flag {:#x}): T = {} != {}",
                    flags[n].bits(),
                    t[n],
                    value(gids[n])
                ));
            }
        }
    }
    // node synchronizer: slaves restored from their masters
    set_garbage(&mut model, &|f| f.is_slave() || f.is_pure_ghost())?;
    ok(
        model.synchronize_nodal(T::Diffusion),
        "synchronize_nodal _diffusion",
    )?;
    {
        let t_g = ok(model.temperature(), "T")?;
        let t = t_g.as_slice();
        for (n, f) in flags.iter().enumerate() {
            if f.is_pure_ghost() {
                continue;
            }
            if t[n] != value(gids[n]) {
                return Err(format!(
                    "node {n} (flag {:#x}): T = {} after _diffusion (nodes)",
                    f.bits(),
                    t[n]
                ));
            }
        }
    }
    // _diffusion_gradient: the ghosts' own gradients after a residual
    // assembly equal the owners' (the nodal values were synchronised)
    ok(model.assemble_residual(), "residual")?;
    if comm.size() == 1 {
        // one rank: no ghost elements, nothing to compare
        ok(
            model.synchronize(T::DiffusionGradient),
            "synchronize _diffusion_gradient",
        )?;
        return ok(
            model.synchronize(T::ConstitutiveLawId),
            "_constitutive_law_id",
        );
    }
    let law = ok(model.law(0), "law")?;
    let before = ok(
        law.base()
            .grad_u
            .get(ElementType::Tetrahedron4, GhostType::Ghost),
        "ghost grad",
    )?
    .as_slice()
    .to_vec();
    ok(
        model.synchronize(T::DiffusionGradient),
        "synchronize _diffusion_gradient",
    )?;
    let law = ok(model.law(0), "law")?;
    let after = ok(
        law.base()
            .grad_u
            .get(ElementType::Tetrahedron4, GhostType::Ghost),
        "ghost grad",
    )?
    .as_slice()
    .to_vec();
    if before.len() != after.len()
        || before
            .iter()
            .zip(&after)
            .any(|(a, b)| (a - b).abs() > 1e-12 * a.abs().max(1.0))
    {
        return Err("ghost gradients differ from the owners' after _diffusion_gradient".into());
    }
    ok(
        model.synchronize(T::ConstitutiveLawId),
        "_constitutive_law_id",
    )?;
    if comm.size() > 1 && model.synchronize(T::SmmStress).is_ok() {
        return Err("an unregistered tag must be refused".into());
    }
    // UD-553 compat: upstream's silent no-op, nothing
    // changed
    model
        .physics
        .upstream_compat
        .unregistered_synchronization_tag_is_a_noop = true;
    ok(model.synchronize(T::SmmStress), "UD-553 compat no-op")?;
    let law = ok(model.law(0), "law")?;
    let still = ok(
        law.base()
            .grad_u
            .get(ElementType::Tetrahedron4, GhostType::Ghost),
        "ghost grad",
    )?
    .as_slice()
    .to_vec();
    if !akantu_validation::exact::exact_slices_eq(&after, &still) {
        return Err("the UD-553 compat no-op changed the ghost gradients".into());
    }
    model
        .physics
        .upstream_compat
        .unregistered_synchronization_tag_is_a_noop = false;
    if model.synchronize_nodal(T::SmmUv).is_ok() && comm.size() > 1 {
        return Err("an unknown DOF tag must be refused".into());
    }
    Ok(())
}

/// UD-275 compat: with `laws_reject_model_tags` the laws refuse the model's
/// `_diffusion` tag as upstream, and the first solve step fails on every
/// rank with upstream's message — agreed before any message, promptly; the
/// default mode solves.
pub fn ud275_compat_reproduces_upstream_failure(comm: Communicator) -> Outcome {
    let (mut model, _shared) = model_on(&comm, 2)?;
    model.physics.upstream_compat = DiffusionUpstreamCompat {
        laws_reject_model_tags: true,
        ..DiffusionUpstreamCompat::default()
    };
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init",
    )?;
    let dt = ok(model.stable_time_step(), "dt")?;
    ok(model.set_time_step(0.5 * dt, None), "set dt")?;
    let t0 = std::time::Instant::now();
    let r = model.solve_step(None);
    let elapsed = t0.elapsed();
    if comm.size() == 1 {
        // one rank: no element synchronizer, nothing exchanged
        return ok(r, "one-rank solve");
    }
    let e = match r {
        Ok(()) => return Err("the compat mode must fail like upstream".into()),
        Err(e) => e.to_string(),
    };
    if !e.contains("Unknown ghost synchronization tag : diffusion")
        && !e.contains("failed on another rank")
    {
        return Err(format!("unexpected error: {e}"));
    }
    if elapsed > std::time::Duration::from_secs(5) {
        return Err(format!("returned after {elapsed:?}"));
    }
    // default mode: the same model solves
    let (mut fixed, _s) = model_on(&comm, 2)?;
    ok(
        fixed.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init fixed",
    )?;
    let dt = ok(fixed.stable_time_step(), "dt")?;
    ok(fixed.set_time_step(0.5 * dt, None), "set dt")?;
    ok(fixed.solve_step(None), "default-mode solve")
}

/// Selector failing on one rank only.
#[derive(Debug)]
pub struct FailOnRank(pub i32);

impl akantu_model::constitutive_law::ConstitutiveLawSelector for FailOnRank {
    fn select(
        &self,
        ctx: &akantu_model::constitutive_law::SelectorContext<'_>,
        _element: akantu_core::Element,
    ) -> Result<Option<usize>> {
        if ctx.mesh.communicator().rank() == self.0 {
            Err(akantu_core::Error::invalid(format!(
                "selector fails on rank {} only",
                self.0
            )))
        } else {
            Ok(Some(0))
        }
    }
}

fn check_agreed(
    comm: &Communicator,
    r: Result<()>,
    own: &str,
    fatal: bool,
    elapsed: std::time::Duration,
) -> Outcome {
    let last = comm.size() as i32 - 1;
    let e = match r {
        Ok(()) => return Err("the initialisation must fail on every rank".into()),
        Err(e) => e,
    };
    let msg = e.to_string();
    if matches!(e, akantu_core::Error::Fatal { .. }) != fatal {
        return Err(format!("fatal = {fatal} expected, got: {msg}"));
    }
    let want = if comm.rank() == last {
        own
    } else {
        "failed on another rank"
    };
    if !msg.contains(want) {
        return Err(format!("rank {}: '{want}' expected in: {msg}", comm.rank()));
    }
    if elapsed > std::time::Duration::from_secs(5) {
        return Err(format!("returned after {elapsed:?}"));
    }
    let mut x = [1u64];
    ok(
        comm.all_reduce(&mut x, akantu_comm::ReduceOp::Sum),
        "all_reduce after",
    )?;
    if x[0] != comm.size() as u64 {
        return Err(format!("all_reduce after the failure: {}", x[0]));
    }
    Ok(())
}

/// A selector failing on the last rank only: agreed recoverable error on
/// every rank (nothing committed); a working selector then initialises the
/// model and it solves.
pub fn rank_specific_selector_failure_is_agreed(comm: Communicator) -> Outcome {
    let (mut model, shared) = model_on(&comm, 2)?;
    model.set_law_selector(Box::new(FailOnRank(comm.size() as i32 - 1)));
    let t0 = std::time::Instant::now();
    let r = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    let own = format!("selector fails on rank {} only", comm.size() - 1);
    check_agreed(&comm, r, &own, false, t0.elapsed())?;
    model.set_law_selector(Box::new(akantu_model::constitutive_law::DefaultSelector(0)));
    ok(model.init_laws(), "init_laws after the failure")?;
    let mesh = ok(shared.read(), "read")?;
    for g in GhostType::ALL {
        let n = mesh.nb_element(ElementType::Tetrahedron4, g);
        let h = ok(model.law(0), "law")?
            .base()
            .filter(ElementType::Tetrahedron4, g)
            .len();
        if h != n {
            return Err(format!("{g:?}: the law holds {h} elements, mesh has {n}"));
        }
    }
    drop(mesh);
    let dt = ok(model.stable_time_step(), "dt")?;
    ok(model.set_time_step(0.5 * dt, None), "set dt")?;
    ok(model.solve_step(None), "solve after the re-initialisation")
}

/// A law whose initialisation fails on the last rank only (a second law
/// without conductivity, UD-285 check): agreed fatal error on every rank.
pub fn rank_specific_init_failure_is_agreed_fatal(comm: Communicator) -> Outcome {
    let (mut model, _shared) = model_on(&comm, 2)?;
    if comm.rank() == comm.size() as i32 - 1 {
        ok(
            model.register_new_law("bad", "heat_diffusion", ""),
            "register",
        )?;
    }
    let t0 = std::time::Instant::now();
    let r = model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass));
    check_agreed(
        &comm,
        r,
        "conductivity must be at least dim x dim",
        true,
        t0.elapsed(),
    )
}

/// `thermal_energy_by_node()` sums
/// `internal_flow[n] · Δt` over the **local and master** nodes (upstream
/// `isLocalOrMasterNode`) and all-reduces, so every global node counts
/// once. The internal flow of a conductor sums to zero (`Σ_a ∇N_a = 0`), so
/// the model's own flow cannot tell the counting apart: the flow is set to
/// 1 on every node of every rank, and the result must be `Δt` times the
/// number of global nodes (counting slave nodes too would add the shared
/// nodes again).
pub fn thermal_energy_by_node_counts_each_node_once(comm: Communicator) -> Outcome {
    let n = 4;
    let m = distributed_cube(&comm, n)?;
    let parser = ok(Parser::parse_str(DAT, "heat.dat"), "parser")?;
    let mut model = ok(
        HeatTransferModel::with_parser(SharedMesh::new(m), None, "heat_transfer", parser),
        "model",
    )?;
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init_full",
    )?;
    let dt = 0.125;
    ok(model.set_time_step(dt, None), "dt")?;
    // the model's time step is read by the solve: one step sets it
    ok(model.solve_step(None), "solve step")?;
    ok(model.internal_flow_mut(), "q")?.as_mut_slice().fill(1.0);
    let e = ok(model.thermal_energy_by_node(), "thermal energy by node")?;
    let want = ((n + 1) * (n + 1) * (n + 1)) as Real * dt;
    if comm.rank() == 0 {
        println!(
            "INFO diffusion thermal_energy_by_node ranks={}: {e:.17e} vs {want:.17e}",
            comm.size()
        );
    }
    if (e - want).abs() > 1e-12 * want {
        return Err(format!(
            "thermal energy by node {e} vs {want} (each global node once)"
        ));
    }
    Ok(())
}

/// Material of the oracle scenario `diffusion_gradient_cube_n3` (probe v5).
pub const DAT_GRADIENT: &str = "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    \
    name = mat\n    density = 2\n    capacity = 3\n    \
    conductivity = [[2.0, 0.3, 0.1], [0.2, 1.5, 0.2], [0.1, 0.4, 1.0]]\n  ]\n]\n";

/// MPI-043 against the oracle (`fixtures/parallel/diffusion_gradient_cube_n3`,
/// probe v5, 3 identical pinned runs): the heat-transfer model on the
/// oracle's Scotch partition of the Tet4 cube (n = 3), the temperature
/// `1 + 2x + 3y² + 5xz` on every local node, upstream's
/// `computeDiffusivityGradU(_not_ghost)`, then `synchronize(_diffusion_gradient)`:
/// the temperature, the not-ghost `∇u` (law-local order), the ghost `∇u`
/// before (never computed: 0) and after the exchange (every ghost element in
/// mesh order) — all bitwise.
pub fn diffusion_gradient_matches_oracle(comm: Communicator) -> Outcome {
    let scenario = "diffusion_gradient_cube_n3";
    let (np, rank) = (comm.size(), comm.rank());
    let fx =
        akantu_validation::Fixture::open_by_id(&format!("parallel/{scenario}/np{np}/rank{rank}"))
            .map_err(|e| e.to_string())?;
    let lf = |name: &str| {
        fx.load_f64(name)
            .map(|f| f.data)
            .map_err(|e| format!("{name}: {e}"))
    };
    let mut m = cube_mesh(comm.clone(), 3, rank == 0)?;
    if np > 1 {
        let parts = if rank == 0 {
            super::par_common::oracle_partition(scenario, np)?
        } else {
            Vec::new()
        };
        super::par_common::distribute_prescribed(&mut m, parts)?;
    } else {
        super::par_common::distribute_rcb(&mut m)?;
    }
    super::par_common::compare_with_oracle(&m, &fx)?;
    let parser = ok(Parser::parse_str(DAT_GRADIENT, "heat.dat"), "parser")?;
    let shared = SharedMesh::new(m);
    let mut model = ok(
        HeatTransferModel::with_parser(shared.clone(), None, "heat_transfer", parser),
        "model",
    )?;
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    ok(model.init_full(&options), "init_full")?;
    let coords = ok(shared.read(), "read")?.nodes().as_slice().to_vec();
    {
        let mut t = ok(model.temperature_mut(), "T")?;
        let t = t.as_mut_slice();
        for (n, tn) in t.iter_mut().enumerate() {
            let (x, y, z) = (coords[3 * n], coords[3 * n + 1], coords[3 * n + 2]);
            *tn = 1. + 2. * x + 3. * y * y + 5. * x * z;
        }
    }
    let bits = |have: &[Real], want: &[Real]| akantu_validation::exact::exact_slices_eq(want, have);
    if !bits(
        ok(model.temperature(), "T")?.as_slice(),
        &lf("model/temperature")?,
    ) {
        return Err("temperature differs from the oracle".into());
    }
    ok(
        model.compute_diffusivity_grad_u(GhostType::NotGhost),
        "computeDiffusivityGradU",
    )?;
    let ty = ElementType::Tetrahedron4;
    let ng = ok(model.law(0), "law")?
        .base()
        .grad_u
        .get(ty, GhostType::NotGhost)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default();
    if !bits(&ng, &lf("law0/grad_u/_tetrahedron_4/not_ghost")?) {
        return Err("not-ghost grad u differs from the oracle".into());
    }
    let ghost_by_element = |model: &HeatTransferModel| -> std::result::Result<Vec<Real>, String> {
        let mesh = ok(shared.read(), "read")?;
        let law = ok(model.law(0), "law")?;
        let a = law.base().grad_u.get(ty, GhostType::Ghost).ok();
        let mut v = Vec::new();
        let ne = mesh.nb_element(ty, GhostType::Ghost);
        if let Some(a) = a {
            let nq = a.len().checked_div(ne).unwrap_or(0);
            let d = a.nb_component();
            for e in 0..ne {
                let el = akantu_core::Element::new(ty, e as Idx, GhostType::Ghost);
                let l = ok(model.law_local_numbering().element(el), "local")?[0];
                let l = usize::try_from(l).map_err(|e| e.to_string())?;
                v.extend_from_slice(&a.as_slice()[l * nq * d..(l + 1) * nq * d]);
            }
        }
        Ok(v)
    };
    // At several ranks the oracle records
    // the ghost gradient (a missing quantity is an error, never an empty
    // comparison); at one rank there is no ghost, nothing is recorded
    let want_before = if np > 1 {
        lf("law0/grad_u_ghost_before/_tetrahedron_4")?
    } else {
        lf("law0/grad_u_ghost_before/_tetrahedron_4").unwrap_or_default()
    };
    if !bits(&ghost_by_element(&model)?, &want_before) {
        return Err("ghost grad u before the exchange differs from the oracle".into());
    }
    ok(
        model.synchronize(SynchronizationTag::DiffusionGradient),
        "synchronize _diffusion_gradient",
    )?;
    let want_after = lf("law0/grad_u_ghost_after/_tetrahedron_4").unwrap_or_default();
    if !bits(&ghost_by_element(&model)?, &want_after) {
        return Err("ghost grad u after _diffusion_gradient differs from the oracle".into());
    }
    if np > 1 && want_after.is_empty() {
        return Err("no ghost gradient recorded".into());
    }
    Ok(())
}
