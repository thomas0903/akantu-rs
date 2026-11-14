// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Collective
// protocol tests (ARCHITECTURE) on the local world: a failure on
// one rank at any model callback of a solve step, or in a model method that
// reduces over ranks (stable time step, energies), is reported on every rank
// — its own error on the failing rank, "failed on another rank" elsewhere —
// promptly, never as a transport timeout; healthy collectives reduce over
// the ranks.

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::{ElementType, Error, GhostType, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::callback::{Delegate, InterceptCallback};

/// The local world's transport timeout: a rank stranded by a protocol
/// defect fails with a timeout error after it (and is counted, see
/// [`on_ranks`]).
const TIMEOUT: Duration = Duration::from_secs(20);

const DAT: &str = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n]\n";

/// Two triangles on the unit square shifted by the rank (independent
/// subdomains: the DOF numbering, norms and reductions are collective).
fn model_on(comm: Communicator, method: AnalysisMethod) -> SolidMechanicsModel {
    let r = comm.rank() as Real;
    let size = comm.size();
    let mut mesh = Mesh::with_communicator(2, "m", comm).unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for p in [[r, 0.], [r + 1., 0.], [r + 1., 1.], [r, 1.]] {
            acc.push_node(&p).unwrap();
        }
        let mut c = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        c.push_tuple(&[0, 1, 2]);
        c.push_tuple(&[0, 2, 3]);
    }
    {
        // element group of both triangles (group energies)
        let mut g = mesh.groups_mut().create_element_group("all", 2).unwrap();
        for e in 0..2 {
            g.add(
                akantu_core::Element::new(ElementType::Triangle3, e, GhostType::NotGhost),
                Some(if e == 0 { &[0, 1, 2] } else { &[0, 2, 3] }),
                true,
            )
            .unwrap();
        }
    }
    let parser = Parser::parse_str(DAT, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    // the Eigen backend refuses parallel systems (as upstream); the faer
    // extension solves the centralised system on rank 0 (D-040)
    let mut options = ModelOptions::new(method);
    if size > 1 {
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    model.init_full(&options).unwrap();
    {
        let mut b = model.blocked_dofs_mut().unwrap();
        let b = b.as_mut_slice();
        b[0] = true;
        b[1] = true;
        b[3] = true;
    }
    if method != AnalysisMethod::Static {
        model.velocity_mut().unwrap().as_mut_slice()[2] = 1e-3;
        let dt = 0.5 * model.stable_time_step().unwrap();
        model.set_time_step(dt, None).unwrap();
    }
    model
}

fn remote(e: &Error) -> bool {
    matches!(e, Error::Communication(m) if m.contains("failed on another rank"))
}

fn own(e: &Error) -> bool {
    match e {
        Error::User(m) => m.contains("injected"),
        Error::InvalidArgument(m) => m.contains("has no material"),
        // A user-resized array, a missing group
        Error::DimensionMismatch { .. } | Error::NotFound { .. } => true,
        _ => false,
    }
}

fn class<T>(r: &Result<T>) -> &'static str {
    match r {
        Ok(_) => "ok",
        Err(e) if remote(e) => "remote",
        Err(e) if own(e) => "own",
        Err(e) => panic!("unexpected error {e:?}"),
    }
}

/// Run `f` on every rank of a fresh local world and require that **no wait
/// of the world ended at its timeout** — the failure was agreed by the
/// protocol, not by a stranded rank's timeout. The count covers timeouts a
/// rank masked by returning its own error in preference, which the error
/// classes alone would not show. (this replaced a 5 s bound on the
/// world's wall time, which a stalled process exceeds without any timeout.)
fn on_ranks<T: Send>(n: usize, f: impl Fn(Communicator) -> T + Sync) -> Vec<T> {
    let out = run_local(n, TIMEOUT, |comm| {
        let probe = comm.clone();
        let v = f(comm);
        // the world's count when this rank is done; the last rank sees all
        (v, probe.local_timeouts())
    });
    let timeouts = out.iter().map(|(_, t)| t.unwrap_or(u64::MAX)).max();
    assert_eq!(
        timeouts,
        Some(0),
        "a rank waited for a timeout: {timeouts:?} wait(s) ended at the {TIMEOUT:?} timeout"
    );
    out.into_iter().map(|(v, _)| v).collect()
}

/// User callback failing at the model callback `name` (on the ranks where
/// `fail` is set), delegating otherwise.
struct FailAt {
    name: &'static str,
    fail: bool,
}

impl FailAt {
    fn check(&self, what: &str) -> Result<()> {
        if self.fail && self.name == what {
            Err(Error::User(format!("injected failure in {what}")))
        } else {
            Ok(())
        }
    }
}

impl InterceptCallback for FailAt {
    fn assemble_matrix(&mut self, model: &mut Delegate<'_>, id: &str) -> Result<()> {
        model.assemble_matrix(id)?;
        self.check(&format!("assemble_matrix:{id}"))
    }
    fn assemble_lumped_matrix(&mut self, model: &mut Delegate<'_>, id: &str) -> Result<()> {
        model.assemble_lumped_matrix(id)?;
        self.check(&format!("assemble_lumped_matrix:{id}"))
    }
    fn assemble_residual(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.assemble_residual()?;
        self.check("assemble_residual")
    }
    fn predictor(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.predictor()?;
        self.check("predictor")
    }
    fn corrector(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.corrector()?;
        self.check("corrector")
    }
    fn before_solve_step(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.before_solve_step()?;
        self.check("before_solve_step")
    }
    fn after_solve_step(&mut self, model: &mut Delegate<'_>, converged: bool) -> Result<()> {
        model.after_solve_step(converged)?;
        self.check("after_solve_step")
    }
}

fn callback_failures(method: AnalysisMethod, callbacks: &[&'static str]) {
    for n in [1usize, 2, 4] {
        for &name in callbacks {
            for failing in 0..n {
                let out = on_ranks(n, |comm| {
                    let rank = comm.rank() as usize;
                    let mut model = model_on(comm, method);
                    // a healthy step first (matrices, lumped mass assembled)
                    model.solve_step(None).unwrap();
                    let mut cb = FailAt {
                        name,
                        fail: rank == failing,
                    };
                    let r = model.solve_step_intercepted(&mut cb, None);
                    (rank, class(&r))
                });
                for (rank, c) in out {
                    let want = if rank == failing { "own" } else { "remote" };
                    assert_eq!(
                        c, want,
                        "{method}: {name} failing on rank {failing}/{n}: rank {rank} got {c}"
                    );
                }
            }
        }
    }
}

#[test]
fn explicit_lumped_callbacks_rank_specific_failure() {
    callback_failures(
        AnalysisMethod::ExplicitLumpedMass,
        &[
            "before_solve_step",
            "predictor",
            "assemble_lumped_matrix:M",
            "assemble_residual",
            "corrector",
            "after_solve_step",
        ],
    );
}

#[test]
fn implicit_dynamic_callbacks_rank_specific_failure() {
    callback_failures(
        AnalysisMethod::ImplicitDynamic,
        &[
            "before_solve_step",
            "predictor",
            "assemble_matrix:K",
            "assemble_matrix:M",
            "assemble_residual",
            "corrector",
            "after_solve_step",
        ],
    );
}

#[test]
fn explicit_consistent_callbacks_rank_specific_failure() {
    // Consistent mass (`assemble_matrix:M` inside an
    // explicit step, centralised faer solve at 2/4 ranks)
    callback_failures(
        AnalysisMethod::ExplicitConsistentMass,
        &[
            "before_solve_step",
            "predictor",
            "assemble_matrix:M",
            "assemble_residual",
            "corrector",
            "after_solve_step",
        ],
    );
}

#[test]
fn static_callbacks_rank_specific_failure() {
    callback_failures(
        AnalysisMethod::Static,
        &[
            "before_solve_step",
            "predictor",
            "assemble_matrix:K",
            "assemble_residual",
            "corrector",
            "after_solve_step",
        ],
    );
}

/// Model methods that reduce over ranks: a rank whose local part fails
/// (element without material: stable time step, lumped mass of the kinetic
/// energy) reports its error, the others the agreed remote failure.
#[test]
fn model_reductions_rank_specific_failure() {
    for n in [1usize, 2, 4] {
        for failing in 0..n {
            let out = on_ranks(n, |comm| {
                let rank = comm.rank() as usize;
                let mut model = model_on(comm, AnalysisMethod::ExplicitLumpedMass);
                model.solve_step(None).unwrap();
                if rank == failing {
                    model
                        .material_by_element_mut()
                        .get_mut(ElementType::Triangle3, GhostType::NotGhost)
                        .unwrap()
                        .as_mut_slice()[1] = -1;
                    model.force_mass_reassembly();
                } else {
                    model.force_mass_reassembly();
                }
                let dt = model.stable_time_step();
                let ek = model.kinetic_energy();
                (rank, class(&dt), class(&ek))
            });
            for (rank, dt, ek) in out {
                let want = if rank == failing { "own" } else { "remote" };
                assert_eq!(
                    (dt, ek),
                    (want, want),
                    "rank {rank} of {n}, failing {failing}"
                );
            }
        }
    }
}

/// Healthy reductions: identical subdomains give `n ×` the serial energies
/// (rank-ordered sums of identical values are exact) and the serial stable
/// time step (minimum).
#[test]
fn model_reductions_sum_and_min_over_ranks() {
    let serial = on_ranks(1, |comm| {
        let mut m = model_on(comm, AnalysisMethod::ExplicitLumpedMass);
        m.solve_step(None).unwrap();
        (
            m.stable_time_step().unwrap(),
            m.energy("kinetic").unwrap(),
            m.energy("potential").unwrap(),
            m.energy("external work").unwrap(),
        )
    })[0];
    for n in [2usize, 4] {
        let out = on_ranks(n, |comm| {
            let mut m = model_on(comm, AnalysisMethod::ExplicitLumpedMass);
            m.solve_step(None).unwrap();
            (
                m.stable_time_step().unwrap(),
                m.energy("kinetic").unwrap(),
                m.energy("potential").unwrap(),
                m.energy("external work").unwrap(),
            )
        });
        let nf = n as Real;
        for (dt, ek, ep, ew) in out {
            assert_eq!(dt.to_bits(), serial.0.to_bits());
            // sums of n identical values in rank order
            let sum = |x: Real| (1..n).fold(x, |a, _| a + x);
            assert_eq!(ek.to_bits(), sum(serial.1).to_bits(), "kinetic, {n} ranks");
            assert_eq!(
                ep.to_bits(),
                sum(serial.2).to_bits(),
                "potential, {n} ranks"
            );
            assert_eq!(
                ew.to_bits(),
                sum(serial.3).to_bits(),
                "external work, {n} ranks"
            );
            assert!((ek - nf * serial.1).abs() <= 1e-15 * ek.abs());
        }
    }
}

/// The remaining reductions — potential energy (a law
/// whose internal was resized on one rank), external work (a resized
/// `external_force`), the group energy (a group missing on one rank) and
/// the consistent kinetic energy (a resized velocity).
#[test]
fn model_energy_reductions_rank_specific_failure() {
    for n in [1usize, 2, 4] {
        for failing in 0..n {
            let out = on_ranks(n, |comm| {
                let rank = comm.rank() as usize;
                let bad = rank == failing;
                let mut model = model_on(comm.clone(), AnalysisMethod::ExplicitLumpedMass);
                model.solve_step(None).unwrap();
                let saved_stress = model
                    .material(0)
                    .unwrap()
                    .base()
                    .stress
                    .get(ElementType::Triangle3, GhostType::NotGhost)
                    .unwrap()
                    .clone();
                if bad {
                    model
                        .material_mut(0)
                        .unwrap()
                        .base_mut()
                        .stress
                        .get_mut(ElementType::Triangle3, GhostType::NotGhost)
                        .unwrap()
                        .try_resize(1, 0.0)
                        .unwrap();
                }
                let ep = model.energy("potential");
                if bad {
                    model
                        .material_mut(0)
                        .unwrap()
                        .base_mut()
                        .stress
                        .get_mut(ElementType::Triangle3, GhostType::NotGhost)
                        .unwrap()
                        .replace(saved_stress);
                }
                let f_ext = model.external_force().unwrap().clone();
                if bad {
                    model
                        .external_force_mut()
                        .unwrap()
                        .replace(akantu_core::Array::from_vec(vec![0.0; 2], 2).unwrap());
                }
                let ew = model.energy("external work");
                model.external_force_mut().unwrap().replace(f_ext);
                let group = if bad { "no_such_group" } else { "all" };
                let eg = model.energy_of_group("potential", group);
                // consistent mass: a model with `M` instead of the lumped mass
                let mut cm = model_on(comm, AnalysisMethod::ExplicitConsistentMass);
                cm.solve_step(None).unwrap();
                if bad {
                    cm.velocity_mut()
                        .unwrap()
                        .replace(akantu_core::Array::from_vec(vec![0.0; 2], 2).unwrap());
                }
                let ek = cm.energy("kinetic");
                (rank, class(&ep), class(&ew), class(&eg), class(&ek))
            });
            for (rank, ep, ew, eg, ek) in out {
                let want = if rank == failing { "own" } else { "remote" };
                assert_eq!(
                    (ep, ew, eg, ek),
                    (want, want, want, want),
                    "rank {rank} of {n}, failing {failing}"
                );
            }
        }
    }
}

/// The public assembly entry points are collectives — a
/// rank-specific failure (user-resized displacement; element without a
/// material for the masses) is agreed.
#[test]
fn public_assembly_collectives_rank_specific_failure() {
    type Op = fn(&mut SolidMechanicsModel) -> Result<()>;
    let ops: [(&str, Op); 5] = [
        ("assemble_internal_forces", |m| m.assemble_internal_forces()),
        ("assemble_stiffness_matrix", |m| {
            m.assemble_stiffness_matrix(true)
        }),
        ("assemble_residual", |m| m.assemble_residual()),
        ("assemble_mass_lumped", |m| m.assemble_mass_lumped()),
        ("assemble_mass", |m| m.assemble_mass()),
    ];
    for n in [1usize, 2, 4] {
        for (name, op) in ops {
            for failing in 0..n {
                let out = on_ranks(n, |comm| {
                    let rank = comm.rank() as usize;
                    let mut model = model_on(comm, AnalysisMethod::Static);
                    if rank == failing {
                        if name.starts_with("assemble_mass") {
                            model
                                .material_by_element_mut()
                                .get_mut(ElementType::Triangle3, GhostType::NotGhost)
                                .unwrap()
                                .as_mut_slice()[1] = -1;
                            model.force_mass_reassembly();
                        } else {
                            model
                                .displacement_mut()
                                .unwrap()
                                .replace(akantu_core::Array::from_vec(vec![0.0; 2], 2).unwrap());
                        }
                    }
                    let r = op(&mut model);
                    (rank, class(&r))
                });
                for (rank, c) in out {
                    let want = if rank == failing { "own" } else { "remote" };
                    assert_eq!(c, want, "{name} failing on rank {failing}/{n}: rank {rank}");
                }
            }
        }
    }
}

/// A nodal array resized on one rank only fails the solve
/// step on that rank and is agreed everywhere else (no panic, no stall).
#[test]
fn resized_nodal_array_on_one_rank_is_agreed() {
    for method in [
        AnalysisMethod::ExplicitLumpedMass,
        AnalysisMethod::Static,
        AnalysisMethod::ImplicitDynamic,
    ] {
        for n in [1usize, 2, 4] {
            for failing in 0..n {
                let out = on_ranks(n, |comm| {
                    let rank = comm.rank() as usize;
                    let mut model = model_on(comm, method);
                    model.solve_step(None).unwrap();
                    if rank == failing {
                        model
                            .external_force_mut()
                            .unwrap()
                            .replace(akantu_core::Array::from_vec(vec![0.0; 2], 2).unwrap());
                    }
                    let r = model.solve_step(None);
                    (rank, class(&r))
                });
                for (rank, c) in out {
                    let want = if rank == failing { "own" } else { "remote" };
                    assert_eq!(c, want, "{method}: failing {failing}/{n}: rank {rank}");
                }
            }
        }
    }
}
