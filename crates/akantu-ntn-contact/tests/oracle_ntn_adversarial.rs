// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential replay
// of the adversarial oracle cases fixtures/model/ntn_contact/adv_*
// (oracle/probes/ntn_contact_adv/ntn_adv_probe.cc, unmodified upstream
// traction-at-split-node-contact classes; written independently of the
// port). Every recorded value is compared by the validation
// harness under the case's rule of tolerances.json (exact rule NTN-BIT; NaN
// and signed-zero semantics are the comparator's, D-046 / D-051).
//
//   adv_stale_position      displacement written between steps vs the
//                           release-cached current position
//   adv_3d_ras_reversal_dc  3-D, R&S regularized Dieterich ageing +
//                           Rubin-Ampuero: stick, slip, stick, reversed slip
//   adv_3d_ras_reversal     the same with an unstable explicit-Euler ageing
//                           update: upstream strengths turn NaN from step 96,
//                           the corrected mode stops there (UD-039)
//   adv_restart_continue    restart mid-run and continue (UD-394 / UD-395)

mod common;

use akantu_core::Real;
use akantu_core::enums::SpatialDirection;
use akantu_model::boundary_condition::{FlagOnly, FromSameDim};
use akantu_ntn_contact::manual_restart::{dump_array, dump_restart, load_array, load_restart};
use akantu_ntn_contact::{ModelState, NtnContactSolverCallback, TasnUpstreamCompat};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::Fixture;
use common::oracle::*;
use common::*;

fn parser(f: &Fixture) -> Parser {
    Parser::parse_file(input(f, "material")).unwrap()
}

fn fixed_bcs_2d(model: &mut SolidMechanicsModel) {
    let x = FlagOnly {
        axis: SpatialDirection::X,
    };
    let y = FlagOnly {
        axis: SpatialDirection::Y,
    };
    model.apply_dirichlet_bc(&y, "base_bottom").unwrap();
    model.apply_dirichlet_bc(&x, "base_bottom").unwrap();
    model.apply_dirichlet_bc(&x, "base_left").unwrap();
    model.apply_dirichlet_bc(&x, "base_right").unwrap();
}

fn positions(model: &SolidMechanicsModel) -> Vec<Real> {
    let mesh_h = model.physics.base.mesh.clone();
    let m = mesh_h.read().unwrap();
    m.nodes().as_slice().to_vec()
}

/// (normals, gap, current position) after `updateNormals` +
/// `computeNormalGap`, as the probe records them.
fn normals_and_gap(
    model: &mut SolidMechanicsModel,
    solver: &mut NtnContactSolverCallback,
) -> (Vec<Real>, Vec<Real>, Vec<Real>) {
    let mesh_h = model.physics.base.mesh.clone();
    let m = mesh_h.read().unwrap();
    let st = ModelState::of_model(&m, model).unwrap();
    solver.contact_mut().update_normals(&st).unwrap();
    let gap = solver.contact().compute_normal_gap(&st).unwrap();
    let cp = st.current_position().unwrap().to_vec();
    (solver.contact().normals().as_slice().to_vec(), gap, cp)
}

fn stale_position(test: &str, compat: TasnUpstreamCompat) {
    let (f, mut r) = Replay::new(test, "adv_stale_position");
    let mesh = read_mesh(&input(&f, "mesh"), 2);
    let mut model = model(mesh, parser(&f));
    let mut solver =
        NtnContactSolverCallback::new(&mut model, "slider_bottom", "base_top", 1, 0.1, compat)
            .unwrap();
    r.check("time_step", &[model.time_step(None).unwrap()]);
    let x = positions(&model);
    let nn = x.len() / 2;
    // write 0, before any step: the cached current position is still X
    {
        let mut d_g = model.displacement_mut().unwrap();
        let d = d_g.as_mut_slice();
        for n in 0..nn {
            d[2 * n] = 1e-4 * x[2 * n + 1];
            d[2 * n + 1] = 2e-3 * x[2 * n] * x[2 * n] - 1e-5;
        }
    }
    let (nrm, gap, cp) = normals_and_gap(&mut model, &mut solver);
    r.check("w0/normals", &nrm);
    r.check("w0/gap", &gap);
    r.check("w0/current_position", &cp);
    fixed_bcs_2d(&mut model);
    model
        .apply_neumann_bc(&FromSameDim::new(vec![0.0, -1e6]), "slider_top")
        .unwrap();
    let mut rec = Series::default();
    for _ in 0..30 {
        solver.solve_step(&mut model, None).unwrap();
        rec.push_state("", solver.contact(), solver.friction());
    }
    // write 1: seen (the corrector changed the release since the last refresh)
    {
        let mut d_g = model.displacement_mut().unwrap();
        let d = d_g.as_mut_slice();
        for n in 0..nn {
            d[2 * n + 1] += 1e-4 * x[2 * n];
        }
    }
    let (nrm, gap, _) = normals_and_gap(&mut model, &mut solver);
    r.check("w1/normals", &nrm);
    r.check("w1/gap", &gap);
    // write 2: not seen (no release change since the refresh of write 1)
    {
        let mut d_g = model.displacement_mut().unwrap();
        let d = d_g.as_mut_slice();
        for n in 0..nn {
            d[2 * n + 1] += 3e-4 * x[2 * n] * x[2 * n];
        }
    }
    let (nrm, gap, cp) = normals_and_gap(&mut model, &mut solver);
    r.check("w2/normals", &nrm);
    r.check("w2/gap", &gap);
    r.check("w2/current_position", &cp);
    model
        .apply_neumann_bc(&FromSameDim::new(vec![4e3, -1e6]), "slider_top")
        .unwrap();
    for _ in 30..60 {
        solver.solve_step(&mut model, None).unwrap();
        rec.push_state("", solver.contact(), solver.friction());
    }
    rec.compare(&mut r, "");
    r.check(
        "final/displacement",
        model.displacement().unwrap().as_slice(),
    );
    r.check("final/velocity", model.velocity().unwrap().as_slice());
    r.check("final/normals", solver.contact().normals().as_slice());
    r.finish();
}

#[test]
fn oracle_adv_stale_position_upstream_mode() {
    stale_position(
        "ntn_contact::oracle_adv_stale_position_upstream_mode",
        TasnUpstreamCompat::upstream(),
    );
}

#[test]
fn oracle_adv_stale_position_corrected_mode() {
    stale_position(
        "ntn_contact::oracle_adv_stale_position_corrected_mode",
        TasnUpstreamCompat::default(),
    );
}

/// Replay of a 3-D rate-and-state case; returns the step at which the
/// corrected mode stopped with its domain error (`None`: ran to the end).
fn ras_3d(case_name: &str, compat: TasnUpstreamCompat) -> Option<usize> {
    let (f, mut r) = Replay::new(&format!("ntn_contact::ras_3d::{case_name}"), case_name);
    let mesh = read_mesh(&input(&f, "mesh"), 3);
    let mut model = model(mesh, parser(&f));
    let mut solver =
        NtnContactSolverCallback::new(&mut model, "slider_bottom", "base_top", 1, 0.1, compat)
            .unwrap();
    r.check("time_step", &[model.time_step(None).unwrap()]);
    r.check("normals", solver.contact().normals().as_slice());
    r.check("impedance", solver.contact().impedance().as_slice());
    for axis in [
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ] {
        model
            .apply_dirichlet_bc(&FlagOnly { axis }, "base_bottom")
            .unwrap();
    }
    let mut rec = Series::default();
    model
        .apply_neumann_bc(&FromSameDim::new(vec![0.0, 1e6, 0.0]), "slider_top")
        .unwrap();
    let nb = solver.contact().nb_contact_nodes();
    for s in 0..200 {
        if s == 40 {
            model
                .apply_neumann_bc(&FromSameDim::new(vec![4e5, 0.0, 2e5]), "slider_top")
                .unwrap();
        } else if s == 100 {
            model
                .apply_neumann_bc(&FromSameDim::new(vec![-9e5, 0.0, -4.5e5]), "slider_top")
                .unwrap();
        }
        if let Err(e) = solver.solve_step(&mut model, None) {
            // corrected mode: the UD-039 domain error where upstream lets a
            // NaN strength through; the oracle has a NaN strength at a node
            // in contact at this very step, and every earlier step matches
            assert!(!compat.rate_and_state_non_finite_propagates, "{e}");
            let msg = e.to_string();
            // The cause is the unstable explicit-Euler ageing update
            assert!(
                msg.contains("UD-039") && msg.contains("negative state"),
                "{msg}"
            );
            eprintln!("{case_name}: corrected mode stops at step {s}: {msg}");
            let fs = f64s(&f, "frictional_strength");
            let ic = f64s(&f, "is_in_contact");
            assert!(
                (0..nb).any(|n| fs[s * nb + n].is_nan() && ic[s * nb + n] == 1.0),
                "{case_name}: the oracle has no NaN strength at a contact node at step {s}"
            );
            rec.compare_rows(&mut r, s);
            r.finish();
            return Some(s);
        }
        rec.push_state("", solver.contact(), solver.friction());
        rec.push("external_work", &solver.external_work());
        rec.push(
            "energies",
            &[
                model.energy("kinetic").unwrap(),
                model.energy("potential").unwrap(),
                model.energy("external work new").unwrap(),
            ],
        );
    }
    rec.compare(&mut r, "");
    r.check(
        "final/displacement",
        model.displacement().unwrap().as_slice(),
    );
    r.check("final/velocity", model.velocity().unwrap().as_slice());
    r.check(
        "final/global_friction_traction",
        solver.friction().global_friction_traction().as_slice(),
    );
    r.finish();
    // the scenario does what it is meant to exercise: slip in both
    // directions (τ_x changes sign between steps 99 and 199 at some node)
    let ft = &rec.0["friction_traction"];
    assert!(
        (0..nb).any(|n| ft[(99 * nb + n) * 3] * ft[(199 * nb + n) * 3] < 0.0),
        "{case_name}: no slip reversal"
    );
    None
}

#[test]
fn oracle_adv_3d_ras_reversal_upstream_mode() {
    assert_eq!(
        ras_3d("adv_3d_ras_reversal_dc", TasnUpstreamCompat::upstream()),
        None
    );
    assert_eq!(
        ras_3d("adv_3d_ras_reversal", TasnUpstreamCompat::upstream()),
        None
    );
}

#[test]
fn oracle_adv_3d_ras_reversal_corrected_mode() {
    assert_eq!(
        ras_3d("adv_3d_ras_reversal_dc", TasnUpstreamCompat::default()),
        None
    );
    // the unstable ageing update: stops where the oracle turns NaN
    assert_eq!(
        ras_3d("adv_3d_ras_reversal", TasnUpstreamCompat::default()),
        Some(96)
    );
}

fn restart_bcs(model: &mut SolidMechanicsModel) {
    fixed_bcs_2d(model);
    model
        .apply_neumann_bc(&FromSameDim::new(vec![0.0, -1e6]), "slider_top")
        .unwrap();
}

/// max |a − b| / max |b|.
fn max_rel(rust: &[f64], oracle: &[f64]) -> f64 {
    let scale = oracle.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    rust.iter()
        .zip(oracle)
        .fold(0.0_f64, |m, (a, b)| m.max((a - b).abs()))
        / scale
}

fn restart_continue(test: &str, compat: TasnUpstreamCompat) {
    let (f, mut r) = Replay::new(test, "adv_restart_continue");
    let dir = std::env::temp_dir().join(format!(
        "ntn-adv-restart-{}-{}",
        std::process::id(),
        compat.restart_read_ignores_values
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let prefix = dir.join("ntn").to_string_lossy().into_owned();
    let mut rec = Series::default();
    // run A: 100 steps, restart files written by the port after step 59
    {
        let mesh = read_mesh(&input(&f, "mesh"), 2);
        let mut model = model(mesh, parser(&f));
        let mut solver =
            NtnContactSolverCallback::new(&mut model, "slider_bottom", "base_top", 1, 0.1, compat)
                .unwrap();
        restart_bcs(&mut model);
        for s in 0..100 {
            if s == 20 {
                model
                    .apply_neumann_bc(&FromSameDim::new(vec![3e5, 0.0]), "slider_top")
                    .unwrap();
            }
            solver.solve_step(&mut model, None).unwrap();
            if s == 59 {
                solver.contact().dump_restart(&prefix).unwrap();
                {
                    let (c, fr) = solver.parts_mut();
                    fr.dump_restart(c, &prefix).unwrap();
                }
                dump_restart(&model, dir.join("displacement.restart")).unwrap();
                dump_array(&model.velocity().unwrap(), dir.join("velocity.restart")).unwrap();
                dump_array(
                    &model.acceleration().unwrap(),
                    dir.join("acceleration.restart"),
                )
                .unwrap();
                rec.push_state("a_at_restart/", solver.contact(), solver.friction());
            }
            if s >= 60 {
                rec.push_state("a/", solver.contact(), solver.friction());
                rec.push("a/external_work", &solver.external_work());
            }
        }
        rec.push(
            "a/final_displacement",
            model.displacement().unwrap().as_slice(),
        );
    }
    rec.compare(&mut r, "a");
    // the port's restart files are byte-identical to the oracle's
    let mut n_files = 0;
    for e in std::fs::read_dir(&dir).unwrap() {
        let p = e.unwrap().path();
        let fname = p.file_name().unwrap().to_string_lossy().into_owned();
        let oracle = std::fs::read(input(&f, &format!("restart/{fname}"))).unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), oracle, "restart file {fname}");
        n_files += 1;
    }
    let expected = f
        .manifest()
        .inputs
        .iter()
        .filter(|i| i.role.starts_with("restart/"))
        .count();
    assert_eq!(n_files, expected, "restart file set");
    // run B: fresh model, restart, continue 40 steps
    {
        let mesh = read_mesh(&input(&f, "mesh"), 2);
        let mut model = model(mesh, parser(&f));
        let mut solver =
            NtnContactSolverCallback::new(&mut model, "slider_bottom", "base_top", 1, 0.1, compat)
                .unwrap();
        restart_bcs(&mut model);
        load_restart(&mut model, dir.join("displacement.restart")).unwrap();
        // `load_array` fills a plain array: through copies of the
        // (exportable) model arrays (same values)
        for (get, file) in [(0, "velocity.restart"), (1, "acceleration.restart")] {
            let mut a = if get == 0 {
                model.velocity_mut().unwrap()
            } else {
                model.acceleration_mut().unwrap()
            };
            let mut plain = akantu_core::Array::clone(&a);
            load_array(&mut plain, dir.join(file)).unwrap();
            a.copy_from(&plain).unwrap();
        }
        solver.contact_mut().read_restart(&prefix).unwrap();
        {
            let (c, fr) = solver.parts_mut();
            fr.read_restart(c, &prefix).unwrap();
        }
        model
            .apply_neumann_bc(&FromSameDim::new(vec![3e5, 0.0]), "slider_top")
            .unwrap();
        rec.push_state("b_after_read/", solver.contact(), solver.friction());
        for _ in 60..100 {
            solver.solve_step(&mut model, None).unwrap();
            rec.push_state("b/", solver.contact(), solver.friction());
            rec.push("b/external_work", &solver.external_work());
        }
        rec.push(
            "b/final_displacement",
            model.displacement().unwrap().as_slice(),
        );
    }
    if compat.restart_read_ignores_values {
        // upstream (UD-394: no value restored): run B bitwise
        rec.compare(&mut r, "b");
    } else {
        // corrected (values restored): the restarted run is closer to the
        // uninterrupted oracle run A than upstream's restarted run B is
        for q in [
            "frictional_strength",
            "cumulative_slip",
            "slip",
            "friction_traction",
            "final_displacement",
        ] {
            let rust_b = &rec.0[&format!("b/{q}")];
            let oracle_a = f64s(&f, &format!("a/{q}"));
            let oracle_b = f64s(&f, &format!("b/{q}"));
            let corrected = max_rel(rust_b, &oracle_a);
            let upstream = max_rel(&oracle_b, &oracle_a);
            assert!(
                corrected < upstream,
                "{q}: corrected restart {corrected:e} vs upstream restart {upstream:e}"
            );
        }
    }
    r.finish();
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn oracle_adv_restart_continue_upstream_mode() {
    restart_continue(
        "ntn_contact::oracle_adv_restart_continue_upstream_mode",
        TasnUpstreamCompat::upstream(),
    );
}

#[test]
fn oracle_adv_restart_continue_corrected_mode() {
    restart_continue(
        "ntn_contact::oracle_adv_restart_continue_corrected_mode",
        TasnUpstreamCompat::default(),
    );
}
