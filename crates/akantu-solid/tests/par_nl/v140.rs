// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: adversarial cases of the distributed
// non-local laws (no upstream counterpart), shared by
// non_local_parallel.rs (local world) and mpi_smm.rs (real MPI).

//! The cases v140_a, v140_b and v140_c.

use std::collections::BTreeMap;
use std::time::Duration;

use akantu_comm::{Communicator, ReduceOp, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType};
use akantu_core::{GhostType, Idx, Real};
use akantu_io::read_mesh;
use akantu_mesh::Mesh;
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::InternalFieldRef;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::SolverCallback;
use akantu_validation::Fixture;

use super::Outcome;

type PointId = (Vec<i64>, Idx);

/// Values by point (or node) identity.
type ById = BTreeMap<PointId, Vec<Real>>;

fn ok<T>(r: akantu_core::Result<T>, what: &str) -> Result<T, String> {
    r.map_err(|e| format!("{what}: {e}"))
}

fn mesh_path(case: &str) -> Result<std::path::PathBuf, String> {
    let fx =
        Fixture::open_by_id(&format!("parallel/{case}/np1/rank0")).map_err(|e| e.to_string())?;
    fx.input_path("mesh").map_err(|e| e.to_string())
}

fn mesh_on(comm: &Communicator, path: &std::path::Path, distribute: bool) -> Result<Mesh, String> {
    let mut m = ok(Mesh::with_communicator(2, "mesh", comm.clone()), "mesh")?;
    if comm.rank() == 0 || !distribute {
        ok(read_mesh(&mut m, path, MeshIoType::Gmsh), "read")?;
    }
    if distribute {
        super::par_common::distribute_rcb(&mut m)?;
    }
    Ok(m)
}

fn init(model: &mut SolidMechanicsModel) -> akantu_core::Result<()> {
    model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
}

fn gid(mesh: &Mesh, n: Idx) -> i64 {
    if mesh.is_distributed() {
        mesh.node_global_id(n)
    } else {
        i64::from(n)
    }
}

/// Internal `name` of every owned point (all laws) by global identity.
fn internal_by_id(
    model: &SolidMechanicsModel,
    name: &str,
) -> Result<BTreeMap<PointId, Vec<Real>>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let mut out = BTreeMap::new();
    for m in 0..model.nb_materials() {
        let law = ok(model.material(m), "law")?;
        let filter = law.element_filter().clone();
        let mut res: Result<(), String> = Ok(());
        law.for_each_internal(&mut |n, f| {
            if n != name || res.is_err() {
                return;
            }
            if let InternalFieldRef::Real(f) = f {
                for (ty, g, a) in f.values().as_map().iter_all() {
                    if g != GhostType::NotGhost {
                        continue;
                    }
                    let (Ok(a), Ok(fl)) = (a.read(), filter.get(ty, g)) else {
                        continue;
                    };
                    let nc = a.nb_component();
                    let nq = if fl.is_empty() { 0 } else { a.len() / fl.len() };
                    for (k, &el) in fl.as_slice().iter().enumerate() {
                        let e = akantu_core::Element::new(ty, el, g);
                        let mut c: Vec<i64> = match mesh.element_connectivity(e) {
                            Ok(c) => c.iter().map(|&nn| gid(&mesh, nn)).collect(),
                            Err(er) => {
                                res = Err(er.to_string());
                                return;
                            }
                        };
                        c.sort_unstable();
                        for q in 0..nq {
                            let off = (k * nq + q) * nc;
                            out.insert((c.clone(), q as Idx), a.as_slice()[off..off + nc].to_vec());
                        }
                    }
                }
            }
        });
        res?;
    }
    Ok(out)
}

/// Internal force of every owned node by global id.
fn owned_forces(model: &SolidMechanicsModel) -> Result<BTreeMap<PointId, Vec<Real>>, String> {
    let mesh_h = model.physics.base.mesh.clone();
    let mesh = ok(mesh_h.read(), "mesh")?;
    let f = ok(model.internal_force(), "f")?;
    let mut out = BTreeMap::new();
    for (k, ff) in f.as_slice().as_chunks::<2>().0.iter().enumerate() {
        let n = k as Idx;
        if mesh.is_distributed() && !mesh.node_flag(n).is_local_or_master() {
            continue;
        }
        out.insert((vec![gid(&mesh, n)], 0), ff.to_vec());
    }
    Ok(out)
}

/// max |a - s| / max |s| over the keys of `a` (every key must be in `s`),
/// all-reduced (Max); also the global count of compared entries.
fn worst_rel(
    comm: &Communicator,
    a: &BTreeMap<PointId, Vec<Real>>,
    s: &BTreeMap<PointId, Vec<Real>>,
) -> Result<(Real, u64), String> {
    let scale = s.values().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
    let mut worst: Real = 0.;
    for (k, v) in a {
        let r = s
            .get(k)
            .ok_or_else(|| format!("point {k:?} not in the serial run"))?;
        for (x, y) in v.iter().zip(r) {
            let d = (x - y).abs() / if scale > 0. { scale } else { 1. };
            worst = if d.is_nan() {
                Real::INFINITY
            } else {
                worst.max(d)
            };
        }
    }
    let mut w = [worst];
    ok(comm.all_reduce(&mut w, ReduceOp::Max), "max")?;
    let mut n = [a.len() as u64];
    ok(comm.all_reduce(&mut n, ReduceOp::Sum), "sum")?;
    Ok((w[0], n[0]))
}

/// The probe's 10-step prescribed history (`mazars_matches_oracle`):
/// per step (damage by point, internal force by node).
fn mazars_history(model: &mut SolidMechanicsModel) -> Result<Vec<(ById, ById)>, String> {
    let lambdas = [1., 2., 3., 4., 5., 6., 7., 8., 6., 9.];
    let nodes = {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = ok(mesh_h.read(), "mesh")?;
        mesh.nodes().as_slice().to_vec()
    };
    let mut out = Vec::new();
    for lam in lambdas {
        let l = 1e-4 * lam;
        {
            let mut u = ok(model.displacement_mut(), "u")?;
            for (n, uu) in u
                .as_mut_slice()
                .as_chunks_mut::<2>()
                .0
                .iter_mut()
                .enumerate()
            {
                let (x, y) = (nodes[2 * n], nodes[2 * n + 1]);
                uu[0] = l * (0.6 * x + 0.3 * y * y);
                uu[1] = l * (0.2 * x * y - 0.1 * y);
            }
        }
        ok(
            model
                .physics
                .before_solve_step(&mut model.solver.dof_manager),
            "before",
        )?;
        ok(model.assemble_internal_forces(), "assemble")?;
        ok(
            model
                .physics
                .after_solve_step(&mut model.solver.dof_manager, true),
            "after",
        )?;
        out.push((internal_by_id(model, "damage")?, owned_forces(model)?));
    }
    Ok(out)
}

/// Case `v140_a`: UD-1100's corrected default with
/// two neighbourhoods (the probe's `nl_mazars_two` laws and history) on the
/// bisection partition against the serial run: damage of every owned point
/// within the summation-order guard (1e-14), internal force of every owned
/// node within the existing distributed-vs-serial rule (1e-12 relative,
/// NUMERICAL_VALIDATION): after a
/// damaging history the damage law amplifies the reordering (measured
/// 1.5e-14), so the 1e-14 guard of the averaged fields does not apply to
/// the forces.
pub fn two_neighbourhoods_default_vs_serial(comm: Communicator) -> Outcome {
    let path = mesh_path("nl_mazars_two")?;
    let dat = super::mazars_dat("nl_mazars_two");
    let mut serial = super::model(
        mesh_on(&Communicator::serial(), &path, false)?,
        &dat,
        true,
        super::Sequence::Corrected,
    )?;
    ok(init(&mut serial), "serial init")?;
    let hs = mazars_history(&mut serial)?;
    let mut dist = super::model(
        mesh_on(&comm, &path, true)?,
        &dat,
        true,
        super::Sequence::Corrected,
    )?;
    ok(init(&mut dist), "distributed init")?;
    let hd = mazars_history(&mut dist)?;
    let (mut wd, mut wf) = (0.0_f64, 0.0_f64);
    let mut damaged = 0usize;
    for (s, ((dd, fd), (ds, fs))) in hd.iter().zip(&hs).enumerate() {
        let (a, na) = worst_rel(&comm, dd, ds)?;
        let (b, nb) = worst_rel(&comm, fd, fs)?;
        if na as usize != ds.len() {
            return Err(format!(
                "step {s}: {na} owned points over the ranks, {} in serial",
                ds.len()
            ));
        }
        if nb as usize != fs.len() {
            return Err(format!(
                "step {s}: {nb} owned nodes over the ranks, {} in serial",
                fs.len()
            ));
        }
        wd = wd.max(a);
        wf = wf.max(b);
        damaged = ds.values().filter(|v| v[0] > 0.).count();
    }
    if comm.rank() == 0 {
        eprintln!(
            "INFO v140_a np{}: two neighbourhoods, corrected default vs serial: damage {wd:.3e}, internal force {wf:.3e} (relative to max); {damaged} damaged points at the last step",
            comm.size()
        );
    }
    // damage: the summation-order guard of `docs/NUMERICAL_VALIDATION.md` (absolute, damage
    // in [0, 1]); internal forces after a 10-step damaging history: the
    // existing distributed-vs-serial rule (1e-12 relative, "Mesh
    // distribution and parallel SMM"), not the 1e-14 guard (measured
    // 1.5e-14 at 2 ranks: the damage law amplifies the reordering)
    if wd.is_nan() || wf.is_nan() || wd > 1e-14 || wf > 1e-12 {
        return Err(format!(
            "differs from serial: damage {wd:e}, internal force {wf:e}"
        ));
    }
    if damaged == 0 {
        return Err("the history damages no point (the comparison would be vacuous)".into());
    }
    Ok(())
}

/// `Ok(())` when `init_full` succeeds, `Err("INIT-ERR …")` when it fails,
/// `Err("SETUP …")` for anything else.
pub fn compat_init(comm: Communicator, case: &'static str) -> Outcome {
    let path = mesh_path(case).map_err(|e| format!("SETUP {e}"))?;
    let dat = if case == "nl_pair" {
        super::PAIR_DAT.to_owned()
    } else {
        super::mazars_dat(case)
    };
    let mesh = mesh_on(&comm, &path, true).map_err(|e| format!("SETUP {e}"))?;
    let mut model = super::model(mesh, &dat, true, super::Sequence::Upstream)
        .map_err(|e| format!("SETUP {e}"))?;
    let t0 = std::time::Instant::now();
    let r = init(&mut model);
    let dt = t0.elapsed().as_secs_f64();
    match r {
        Ok(()) => Ok(()),
        Err(e) => Err(format!("INIT-ERR ({dt:.2} s) {e}")),
    }
}

/// Case `v140_b`, local world: upstream's sequence
/// with two neighbourhoods on the bisection partition, where upstream
/// aborts or may abort (UD-1100): the outcome of `init_full` is the same on
/// every rank (no hang, no rank-split), and an error names UD-1100.
pub fn agreed_local(case: &'static str, np: usize) {
    let res = run_local(np, Duration::from_secs(120), move |c| compat_init(c, case));
    let errs: Vec<String> = res
        .iter()
        .filter_map(|r| r.as_ref().err().cloned())
        .collect();
    for e in &errs {
        assert!(e.starts_with("INIT-ERR"), "{case} np{np}: {e}");
    }
    assert!(
        errs.is_empty() || errs.len() == np,
        "{case} np{np}: rank-split outcome: {res:?}"
    );
    if errs.is_empty() {
        eprintln!(
            "INFO v140_b {case} np{np} (bisection partition, upstream sequence): init_full completes on every rank"
        );
    } else {
        eprintln!(
            "INFO v140_b {case} np{np} (bisection partition, upstream sequence): agreed error on every rank: {}",
            errs[0]
        );
        assert!(
            errs[0].contains("UD-1100"),
            "{case} np{np}: the error does not name UD-1100: {}",
            errs[0]
        );
    }
}

/// Case `v140_c`: the corrected default on
/// `pair_test.msh` (quadrangles, four integration points per element)
/// against the serial run: `grad_u non local` of every owned point with a
/// per-point field, so the exchanges' per-point slot mapping is compared
/// (unpacking the points of each element in reverse order gives a
/// difference of 9.0e-3).
pub fn multi_point_default_vs_serial(comm: Communicator) -> Outcome {
    let path = mesh_path("nl_pair")?;
    let run =
        |c: &Communicator, distribute: bool| -> Result<BTreeMap<PointId, Vec<Real>>, String> {
            let mut m = super::model(
                mesh_on(c, &path, distribute)?,
                super::PAIR_DAT,
                true,
                super::Sequence::Corrected,
            )?;
            ok(init(&mut m), "init")?;
            super::set_eigen_grad_u(&mut m)?;
            ok(m.assemble_internal_forces(), "assemble")?;
            internal_by_id(&m, "grad_u non local")
        };
    let serial = run(&Communicator::serial(), false)?;
    let dist = run(&comm, true)?;
    let (w, n) = worst_rel(&comm, &dist, &serial)?;
    let nq4 = serial.keys().filter(|(c, _)| c.len() == 4).count();
    if comm.rank() == 0 {
        eprintln!(
            "INFO v140_c np{}: pair_test.msh corrected default vs serial: grad_u non local {w:.3e} (relative to max), {n} owned points, {nq4} quadrangle points in serial",
            comm.size()
        );
    }
    if n as usize != serial.len() {
        return Err(format!(
            "{n} owned points over the ranks, {} in serial",
            serial.len()
        ));
    }
    if nq4 == 0 {
        return Err("no quadrangle point".into());
    }
    if w.is_nan() || w > 1e-14 {
        return Err(format!("grad_u non local differs from serial by {w:e}"));
    }
    Ok(())
}

/// The case `v140_b` under real MPI: [`compat_init`] of every case on this
/// communicator; the outcome must be the same on every rank (all complete,
/// or all fail), and a failure must name UD-1100 on at least one rank.
pub fn ud1100_compat_outcome_is_agreed(comm: Communicator) -> Outcome {
    for case in ["nl_mazars_two", "nl_pair"] {
        let r = compat_init(comm.clone(), case);
        if let Err(e) = &r
            && !e.starts_with("INIT-ERR")
        {
            return Err(format!("{case}: {e}"));
        }
        let mut v = [
            u64::from(r.is_err()),
            u64::from(r.as_ref().err().is_some_and(|e| e.contains("UD-1100"))),
        ];
        ok(comm.all_reduce(&mut v, ReduceOp::Sum), "sum")?;
        let np = comm.size() as u64;
        if v[0] != 0 && v[0] != np {
            return Err(format!(
                "{case}: rank-split outcome ({} of {np} ranks failed)",
                v[0]
            ));
        }
        if v[0] == np && v[1] == 0 {
            return Err(format!("{case}: the agreed error names UD-1100 on no rank"));
        }
        if comm.rank() == 0 {
            eprintln!(
                "INFO v140_b {case} np{np} (bisection partition, upstream sequence): {}",
                if v[0] == 0 {
                    "init_full completes on every rank"
                } else {
                    "agreed error on every rank"
                }
            );
        }
    }
    Ok(())
}
