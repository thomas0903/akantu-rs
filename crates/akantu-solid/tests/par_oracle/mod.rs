// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential scenarios (no upstream counterpart),
// shared by `parallel_oracle.rs` (local world) and `mpi_smm.rs` (real MPI):
// solid mechanics on a mesh distributed with the oracle's Scotch partition
// against the oracle's per-rank records fixtures/parallel/smm_*/np*/rank*.

#![allow(dead_code)]

use akantu_comm::{Communicator, ReductionPolicy};
use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::Fixture;
use akantu_validation::reduce;

use super::par_common;
use super::par_smm::{self, Outcome, Part, ok};

fn fixture(scenario: &str, comm: &Communicator) -> std::result::Result<Fixture, String> {
    Fixture::open_by_id(&format!(
        "parallel/{scenario}/np{}/rank{}",
        comm.size(),
        comm.rank()
    ))
    .map_err(|e| e.to_string())
}

/// Exact comparison with the oracle record (decision D-046).
fn bits_eq(a: &[Real], b: &[Real]) -> bool {
    akantu_validation::exact::exact_slices_eq(b, a)
}

/// Largest `|a − b|` relative to `max |b|`: NaN-propagating
/// (with `f64::max` an all-NaN `a` gave 0 and passed the static
/// `rel <= 1e-12` check), `+∞` on a length mismatch.
fn max_rel(a: &[Real], b: &[Real]) -> Real {
    reduce::max_abs_diff(a, b) / reduce::max_abs(b.iter().copied()).max(Real::MIN_POSITIVE)
}

#[test]
fn max_rel_is_not_satisfied_by_nan() {
    let want = [1.0, -2.0, 0.5];
    assert_eq!(max_rel(&want, &want), 0.0);
    for k in 0..want.len() {
        let mut have = want;
        have[k] = Real::NAN;
        let rel = max_rel(&have, &want);
        let passes = rel <= 1e-12;
        assert!(!passes, "NaN at {k}: rel {rel}");
    }
    let rel = max_rel(&[Real::NAN; 3], &want);
    let passes = rel <= 1e-12;
    assert!(!passes, "all NaN: rel {rel}");
    assert_eq!(max_rel(&want[..2], &want), Real::INFINITY);
}

fn distributed(
    comm: &Communicator,
    scenario: &str,
    n: usize,
) -> std::result::Result<akantu_mesh::Mesh, String> {
    let part = if comm.size() > 1 {
        Part::Prescribed(if comm.rank() == 0 {
            par_common::oracle_partition(scenario, comm.size())?
        } else {
            Vec::new()
        })
    } else {
        Part::Rcb
    };
    par_smm::distributed_cube(comm, n, &part)
}

/// Explicit run (probe `scenario_explicit`): distribution, law ghost
/// filter, time step, every nodal field after 40 steps bitwise; energies
/// per step exactly in the form of the D-039 amendment (`energies_match`:
/// the recorded value is the fold of the port's per-rank contributions in
/// the association observed for the oracle's `MPI_Allreduce`, the port's
/// the rank-order fold of the same contributions).
pub fn explicit_matches(comm: Communicator) -> Outcome {
    explicit_scenario_matches(comm, "smm_explicit_cube_n4", par_smm::Laws::One)
}

/// Two laws (probe `scenario_explicit_two_laws`, version
/// 5): the explicit run with two laws, law 1 in finite deformation, chosen
/// by the checkerboard selector: as [`explicit_matches`] for both laws'
/// filters and the nodal fields, plus every law's `stress`, `grad_u` and
/// (law 1) `piola_kirchhoff_2` on local and ghost elements after the last
/// step bitwise — the ghost values are the owner's of the last
/// `_smm_stress` exchange (MPI-034, finite-deformation branch).
pub fn explicit_two_laws_matches(comm: Communicator) -> Outcome {
    explicit_scenario_matches(
        comm,
        "smm_explicit_two_laws_cube_n4",
        par_smm::Laws::TwoWithFiniteDeformation,
    )
}

fn ghost_dir(ghost: GhostType) -> &'static str {
    if ghost == GhostType::Ghost {
        "ghost"
    } else {
        "not_ghost"
    }
}

fn explicit_scenario_matches(comm: Communicator, scenario: &str, laws: par_smm::Laws) -> Outcome {
    let fx = fixture(scenario, &comm)?;
    let m = distributed(&comm, scenario, 4)?;
    par_common::compare_with_oracle(&m, &fx)?;
    let shared = SharedMesh::new(m);
    let mut model = laws.model(&shared)?;
    ok(
        model.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)),
        "init",
    )?;
    // ghost filters of the laws (canonical rank order on both sides)
    for li in 0..model.physics.materials.nb_laws() {
        let law = ok(model.physics.materials.law(li), "law")?;
        for ghost in GhostType::ALL {
            let g = ghost_dir(ghost);
            let have: Vec<i64> = law
                .base()
                .element_filter
                .get(ElementType::Tetrahedron4, ghost)
                .map(|a| a.as_slice().iter().map(|&v| i64::from(v)).collect())
                .unwrap_or_default();
            let want = fx
                .load_i64(&format!("law{li}/filter/_tetrahedron_4/{g}"))
                .map(|f| f.data)
                .unwrap_or_default();
            if have != want {
                return Err(format!(
                    "law {li} {g} filter differs ({} vs {})",
                    have.len(),
                    want.len()
                ));
            }
        }
    }
    let (coords, nb) = {
        let mm = ok(shared.read(), "read")?;
        (mm.nodes().as_slice().to_vec(), mm.nb_nodes())
    };
    {
        let mut b_g = ok(model.blocked_dofs_mut(), "blocked")?;
        let b = b_g.as_mut_slice();
        for n in 0..nb {
            if coords[3 * n + 2] == 0.0 {
                b[3 * n] = true;
                b[3 * n + 1] = true;
                b[3 * n + 2] = true;
            }
        }
        drop(b_g);
        let mut u = ok(model.displacement_mut(), "u")?;
        let u = u.as_mut_slice();
        for n in 0..nb {
            let (x, y, z) = (coords[3 * n], coords[3 * n + 1], coords[3 * n + 2]);
            u[3 * n] = 1e-3 * z * z * (1.0 + 0.3 * y);
            u[3 * n + 1] = 5e-4 * z * (0.5 - x);
        }
    }
    let stable = ok(model.stable_time_step(), "stable")?;
    let dt = 0.8 * stable;
    let want_dt = fx
        .load_f64("model/time_step")
        .map_err(|e| e.to_string())?
        .data;
    if !bits_eq(&[stable, dt], &want_dt) {
        return Err(format!("time step {stable:e}/{dt:e} vs oracle {want_dt:?}"));
    }
    ok(model.set_time_step(dt, None), "dt")?;
    let mut ek = Vec::new();
    let mut ep = Vec::new();
    // this rank's contributions to the reduced energies
    let mut lk = Vec::new();
    let mut lp = Vec::new();
    for s in 0..40 {
        ok(model.solve_step(None), &format!("step {s}"))?;
        ek.push(ok(model.energy("kinetic"), "kinetic")?);
        ep.push(ok(model.energy("potential"), "potential")?);
        lk.push(ok(model.local_energy("kinetic"), "local kinetic")?);
        lp.push(ok(model.local_energy("potential"), "local potential")?);
    }
    // gathered before any rank-local comparison can return early (every
    // rank reaches this collective)
    let gathered_k = ok(comm.all_gather_v(&lk), "gather kinetic contributions")?;
    let gathered_p = ok(comm.all_gather_v(&lp), "gather potential contributions")?;
    let fields: [(&str, Vec<Real>); 5] = [
        (
            "model/displacement",
            ok(model.displacement(), "u")?.as_slice().to_vec(),
        ),
        (
            "model/velocity",
            ok(model.velocity(), "v")?.as_slice().to_vec(),
        ),
        (
            "model/acceleration",
            ok(model.acceleration(), "a")?.as_slice().to_vec(),
        ),
        ("model/mass", ok(model.mass(), "mass")?.as_slice().to_vec()),
        (
            "model/internal_force",
            ok(model.internal_force(), "f_int")?.as_slice().to_vec(),
        ),
    ];
    for (name, have) in &fields {
        let want = fx.load_f64(name).map_err(|e| e.to_string())?.data;
        if !bits_eq(have, &want) {
            return Err(format!(
                "{name} differs from the oracle (max rel {:.3e})",
                max_rel(have, &want)
            ));
        }
    }
    if laws == par_smm::Laws::TwoWithFiniteDeformation {
        let mut compared = 0usize;
        for li in 0..model.physics.materials.nb_laws() {
            let law = ok(model.physics.materials.law(li), "law")?;
            let b = law.base();
            let mut internals = vec![("stress", &b.stress), ("grad_u", &b.grad_u)];
            if let Some(pk2) = &b.piola_kirchhoff_2 {
                internals.push(("piola_kirchhoff_2", pk2));
            }
            for (id, field) in internals {
                for ghost in GhostType::ALL {
                    let name = format!("law{li}/{id}/_tetrahedron_4/{}", ghost_dir(ghost));
                    let have = field
                        .get(ElementType::Tetrahedron4, ghost)
                        .map(|a| a.as_slice().to_vec())
                        .unwrap_or_default();
                    let want = match fx.load_f64(&name) {
                        Ok(f) => f.data,
                        // an empty filter (no element of this law and
                        // ghost type on the rank): nothing recorded
                        Err(_) if have.is_empty() => continue,
                        Err(e) => return Err(format!("{name}: {e}")),
                    };
                    if !bits_eq(&have, &want) {
                        return Err(format!(
                            "{name} differs from the oracle (max rel {:.3e}, {} vs {} values)",
                            max_rel(&have, &want),
                            have.len(),
                            want.len()
                        ));
                    }
                    compared += have.len();
                }
            }
            if b.finite_deformation != (li == 1) {
                return Err(format!(
                    "law {li}: finite_deformation {}",
                    b.finite_deformation
                ));
            }
        }
        if comm.rank() == 0 {
            println!(
                "INFO {scenario} at {} ranks: {compared} law internal values bitwise on rank 0",
                comm.size()
            );
        }
    }
    let native = comm.reduction_policy() == ReductionPolicy::NativeMpi;
    for (name, have, parts) in [
        ("model/kinetic_energy", &ek, &gathered_k),
        ("model/potential_energy", &ep, &gathered_p),
    ] {
        let want = fx.load_f64(name).map_err(|e| e.to_string())?.data;
        energies_match(name, have, &want, parts, native, comm.rank() == 0)?;
    }
    Ok(())
}

/// The per-step reduced energies of the explicit runs, exactly
/// (D-039 in the form of its amendment; no tolerance). Upstream
/// records `getEnergy`, the `MPI_Allreduce` sum of the ranks'
/// contributions; the port reduces the same contributions in rank order
/// (`ReductionPolicy::RankOrdered`) or with `MPI_Allreduce`
/// (`NativeMpi`). With `parts[r][s]` rank `r`'s contribution at step `s`
/// (`SolidMechanicsModel::local_energy`, gathered), at every step:
///
/// 1. the recorded value equals, bit for bit, the fold of the port's
///    contributions in the association observed for the oracle's Open MPI
///    5.0.7 at this rank count ([`par_common::oracle_allreduce_fold`]; a
///    rank count without an observed association is refused) — so the
///    port's per-rank contributions reproduce upstream's reduced value;
/// 2. rank-ordered policy: the port's value equals the rank-order fold of
///    its contributions bit for bit; `NativeMpi`: the port's value equals
///    the recorded value bit for bit.
///
/// Where the two associations give different bits, the INFO line (when
/// `info`) counts the steps and gives the largest relative difference
/// (diagnostic only).
pub fn energies_match(
    name: &str,
    have: &[Real],
    want: &[Real],
    parts: &[Vec<Real>],
    native: bool,
    info: bool,
) -> Outcome {
    let np = parts.len();
    if want.len() != have.len() {
        return Err(format!(
            "{name}: {} steps, oracle {}",
            have.len(),
            want.len()
        ));
    }
    if let Some((r, p)) = parts
        .iter()
        .enumerate()
        .find(|(_, p)| p.len() != have.len())
    {
        return Err(format!(
            "{name}: rank {r} gathered {} contributions for {} steps",
            p.len(),
            have.len()
        ));
    }
    let mut column = vec![0.0; np];
    for s in 0..have.len() {
        for (c, p) in column.iter_mut().zip(parts) {
            *c = p[s];
        }
        let observed = par_common::oracle_allreduce_fold(&column)
            .map_err(|e| format!("{name} at {np} ranks: {e}"))?;
        if !bits_eq(&[observed], &[want[s]]) {
            return Err(format!(
                "{name} step {s}: the recorded {:e} is not the fold of the port's {np} per-rank \
                 contributions in the association observed for the oracle's MPI_Allreduce \
                 ({observed:e}; D-039)",
                want[s]
            ));
        }
        if native {
            if !bits_eq(&[have[s]], &[want[s]]) {
                return Err(format!(
                    "{name} step {s} (NativeMpi): {:e} vs oracle {:e}",
                    have[s], want[s]
                ));
            }
        } else {
            let chain = par_common::rank_order_fold(&column)
                .ok_or_else(|| format!("{name}: no contribution gathered"))?;
            if !bits_eq(&[chain], &[have[s]]) {
                return Err(format!(
                    "{name} step {s}: the port's {:e} is not the rank-order fold of its \
                     per-rank contributions ({chain:e}; D-039)",
                    have[s]
                ));
            }
        }
    }
    let differing = akantu_validation::exact::exact_mismatch_count(want, have);
    if differing > 0 && info {
        println!(
            "INFO {name} at {np} ranks: {differing} of {} steps differ from the oracle's bits \
             (max rel {:.3e}; rank order vs the oracle's MPI_Allreduce association, every step \
             exact by the folds, D-039)",
            have.len(),
            max_rel(have, want)
        );
    }
    Ok(())
}

/// Static run (probe `scenario_static`): distribution and law filters
/// exact; displacement within the linear-solver tolerance (oracle: MUMPS,
/// port: faer centralised, D-040).
pub fn static_matches(comm: Communicator) -> Outcome {
    let (model, fx) = static_solved(&comm)?;
    let u = ok(model.displacement(), "u")?.as_slice().to_vec();
    let want = fx
        .load_f64("model/displacement")
        .map_err(|e| e.to_string())?
        .data;
    let rel = max_rel(&u, &want);
    println!(
        "INFO static at {} ranks: displacement max rel {rel:.3e} vs oracle (MUMPS)",
        comm.size()
    );
    if rel > 1e-12 {
        return Err(format!("static displacement differs (max rel {rel:.3e})"));
    }
    Ok(())
}

/// The static scenario solved by the port (faer, centralised) on the
/// oracle's partition, with the rank's oracle record.
pub fn static_solved(
    comm: &Communicator,
) -> std::result::Result<(SolidMechanicsModel, Fixture), String> {
    let scenario = "smm_static_cube_n3";
    let fx = fixture(scenario, comm)?;
    let m = distributed(comm, scenario, 3)?;
    par_common::compare_with_oracle(&m, &fx)?;
    let shared = SharedMesh::new(m);
    let parser = ok(Parser::parse_str(par_smm::DAT, "material.dat"), "parser")?;
    let mut model = ok(
        SolidMechanicsModel::with_parser(shared.clone(), None, "smm", parser),
        "model",
    )?;
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    ok(model.init_full(&options), "init")?;
    let (coords, nb) = {
        let mm = ok(shared.read(), "read")?;
        (mm.nodes().as_slice().to_vec(), mm.nb_nodes())
    };
    {
        let mut b_g = ok(model.blocked_dofs_mut(), "blocked")?;
        let b = b_g.as_mut_slice();
        for n in 0..nb {
            if coords[3 * n + 2] == 0.0 {
                b[3 * n] = true;
                b[3 * n + 1] = true;
                b[3 * n + 2] = true;
            }
        }
        drop(b_g);
        let mut f = ok(model.external_force_mut(), "f")?;
        let f = f.as_mut_slice();
        for n in 0..nb {
            if coords[3 * n + 2] == 1.0 {
                f[3 * n] = 1e-3 * (1.0 + coords[3 * n + 1]);
                f[3 * n + 2] = -2e-3;
            }
        }
    }
    ok(model.solve_step(None), "solve")?;
    Ok((model, fx))
}
