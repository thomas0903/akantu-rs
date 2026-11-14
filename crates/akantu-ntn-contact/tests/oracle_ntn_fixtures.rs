// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential
// tests against fixtures/model/ntn_contact/* (oracle/probes/ntn_contact/
// ntn_probe.cc running the upstream traction-at-split-node-contact classes,
// strict reference build). Every recorded value is compared by the
// validation harness under the case's rule of tolerances.json (exact rule
// NTN-BIT; NaN and signed-zero semantics are the comparator's, D-046 /
// D-051; no local NaN handling).
//
// Fixtures: 43 cases generated on the x86_64 oracle snapshot
// (x86_64 oracle, strict build, three identical runs, 2026-11-11).
// The rate-and-state laws evaluate `ln` through the pure-Rust port of the
// oracle's glibc `log`, so every replay, the 1000-step
// rate-and-state ones included, runs and is bitwise on every platform (with
// the host `f64::ln`, macOS 27.2 differed in 3 of the 5000 energy values of
// `ras_dieterich_ageing` by 1 ulp).

mod common;

use akantu_core::Real;
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FlagOnly, FromSameDim};
use akantu_ntn_contact::boundary_conditions::{ForceBasedDirichlet, SpringBc};
use akantu_ntn_contact::parameter_reader::ParameterReader;
use akantu_ntn_contact::{
    ModelState, NtnBaseContact, NtnContactSolverCallback, TasnUpstreamCompat, allocate_friction,
    initialize_ntn_friction,
};
use akantu_parser::{Parser, SearchScope};
use akantu_solid::SolidMechanicsModel;
use akantu_validation::Fixture;
use common::oracle::*;
use common::*;

fn materials_parser(f: &Fixture) -> Parser {
    Parser::parse_file(input(f, "material")).unwrap()
}

fn fixed_bcs(model: &mut SolidMechanicsModel, block_slider_x: bool) {
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
    if block_slider_x {
        model.apply_dirichlet_bc(&x, "slider_left").unwrap();
        model.apply_dirichlet_bc(&x, "slider_right").unwrap();
    }
}

// ---- basics -----------------------------------------------------------

fn replay_basics(name: &str, dim: usize) {
    let (f, mut r) = Replay::new(&format!("ntn_contact::replay_basics::{name}"), name);
    let mesh = read_mesh(&input(&f, "mesh"), dim);
    let mut model = model(mesh, materials_parser(&f));
    let mesh_h = model.physics.base.mesh.clone();
    let mut contact = {
        let m = mesh_h.read().unwrap();
        NtnBaseContact::new_ntn(&m, dim, "contact", TasnUpstreamCompat::upstream()).unwrap()
    };
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model.assemble_mass_lumped().unwrap();
    {
        let m = mesh_h.read().unwrap();
        let st = ModelState::of_model(&m, &mut model).unwrap();
        contact
            .add_surface_pair(&st, "slider_bottom", "base_top", 1)
            .unwrap();
    }
    r.check("impedance_before_time_step", contact.impedance().as_slice());
    {
        let m = mesh_h.read().unwrap();
        let st = ModelState::of_model(&m, &mut model).unwrap();
        contact.update_normals(&st).unwrap();
        contact.update_lumped_boundary(&m).unwrap();
    }
    let stable = model.stable_time_step().unwrap();
    assert_same_finite(
        &format!("{name}: stable time step"),
        stable,
        param_f64(&f, "stable_time_step"),
    );
    model.set_time_step(stable * 0.1, None).unwrap();
    {
        let m = mesh_h.read().unwrap();
        let st = ModelState::of_model(&m, &mut model).unwrap();
        contact.update_impedance(&st).unwrap();
    }
    let slaves: Vec<i64> = contact
        .slaves()
        .as_slice()
        .iter()
        .map(|&s| i64::from(s))
        .collect();
    let masters: Vec<i64> = contact
        .masters()
        .unwrap()
        .as_slice()
        .iter()
        .map(|&s| i64::from(s))
        .collect();
    assert_eq!(slaves, f.load_i64("slaves").unwrap().data, "{name} slaves");
    assert_eq!(
        masters,
        f.load_i64("masters").unwrap().data,
        "{name} masters"
    );
    r.check("normals", contact.normals().as_slice());
    r.check(
        "lumped_boundary_slaves",
        contact.lumped_boundary_slaves().as_slice(),
    );
    r.check(
        "lumped_boundary_masters",
        contact.lumped_boundary_masters().unwrap().as_slice(),
    );
    r.check("impedance", contact.impedance().as_slice());
    r.check("mass", model.mass().unwrap().as_slice());
    r.finish();
}

#[test]
fn oracle_basics_2d() {
    replay_basics("basics_2d", 2);
}

#[test]
fn oracle_basics_3d() {
    replay_basics("basics_3d", 3);
}

// ---- compression ------------------------------------------------------

fn replay_compression(
    test: &str,
    compat: TasnUpstreamCompat,
) -> (
    Fixture,
    Replay,
    SolidMechanicsModel,
    NtnContactSolverCallback,
    Series,
) {
    let (f, mut r) = Replay::new(test, "compression");
    let mesh = read_mesh(&input(&f, "mesh"), 2);
    let parser = materials_parser(&f);
    let tsf: Real = parser
        .user_section()
        .unwrap()
        .get_parameter_value("time_step_factor", SearchScope::Current)
        .unwrap();
    let mut model = model(mesh, parser);
    let mut solver =
        NtnContactSolverCallback::new(&mut model, "slider_bottom", "base_top", 1, tsf, compat)
            .unwrap();
    fixed_bcs(&mut model, true);
    let time_step = model.time_step(None).unwrap();
    r.check("time_step", &[time_step]);
    let nb_steps = (5e-4 / time_step + 1.0) as usize;
    assert_eq!(nb_steps as f64, param_f64(&f, "nb_steps"));
    let period = 5e-5;
    let mut s_rec = Series::default();
    for s in 0..nb_steps {
        let t = s as Real * time_step;
        let trac = if t < period / 2.0 {
            let sn = (t / period * 2.0 * std::f64::consts::PI).sin();
            vec![0.0 * sn, -1e6 * sn]
        } else {
            vec![0.0, 0.0]
        };
        model
            .apply_neumann_bc(&FromSameDim::new(trac), "slider_top")
            .unwrap();
        solver.solve_step(&mut model, None).unwrap();
        s_rec.push_state("", solver.contact(), solver.friction());
        s_rec.push("external_work", &solver.external_work());
        let e = [
            model.energy("kinetic").unwrap(),
            model.energy("potential").unwrap(),
            model.energy("external work new").unwrap(),
        ];
        s_rec.push("energies", &e);
    }
    (f, r, model, solver, s_rec)
}

#[test]
fn oracle_compression_upstream_mode() {
    let (f, mut r, model, solver, rec) = replay_compression(
        "ntn_contact::oracle_compression_upstream_mode",
        TasnUpstreamCompat::upstream(),
    );
    rec.compare(&mut r, "");
    r.check(
        "final/displacement",
        model.displacement().unwrap().as_slice(),
    );
    r.check("final/velocity", model.velocity().unwrap().as_slice());
    r.check(
        "final/acceleration",
        model.acceleration().unwrap().as_slice(),
    );
    r.check(
        "final/global_contact_pressure",
        solver.contact().global_contact_pressure().as_slice(),
    );
    r.check(
        "final/global_friction_traction",
        solver.friction().global_friction_traction().as_slice(),
    );
    // restart files: byte-identical (same file set: Rubin-Ampuero fallback)
    let dir = std::env::temp_dir().join(format!("ntn-oracle-restart-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let prefix = dir.join("ntn").to_string_lossy().into_owned();
    let mut solver = solver;
    solver.contact().dump_restart(&prefix).unwrap();
    {
        let (c, fr) = solver.parts_mut();
        fr.dump_restart(c, &prefix).unwrap();
    }
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
    // UD-394 (compat): a fresh friction reading the files keeps its values
    let mesh_h = model.physics.base.mesh.clone();
    let m = mesh_h.read().unwrap();
    let mut fresh =
        initialize_ntn_friction(solver.contact(), m.nb_nodes(), &model.physics.base.parser)
            .unwrap();
    fresh.read_restart(solver.contact(), &prefix).unwrap();
    r.check(
        "restart_read/frictional_strength",
        fresh.frictional_strength().as_slice(),
    );
    r.check(
        "restart_read/friction_traction",
        fresh.friction_traction().as_slice(),
    );
    r.check(
        "restart_read/cumulative_slip",
        fresh.cumulative_slip().as_slice(),
    );
    r.check("restart_read/slip", fresh.slip().as_slice());
    // manual displacement restart file
    let text = akantu_ntn_contact::manual_restart::array_text(&model.displacement().unwrap());
    assert_eq!(
        text.as_bytes(),
        std::fs::read(input(&f, "manual_restart"))
            .unwrap()
            .as_slice()
    );
    std::fs::remove_dir_all(&dir).ok();
    r.finish();
}

/// The corrected defaults (no_regularisation honoured, UD-039) give the same
/// numbers: Rubin–Ampuero with `t* = 0` is the unregularised pressure.
#[test]
fn oracle_compression_corrected_mode() {
    let (_, mut r, _, _, rec) = replay_compression(
        "ntn_contact::oracle_compression_corrected_mode",
        TasnUpstreamCompat::default(),
    );
    rec.compare(&mut r, "");
    r.finish();
}

// ---- rate and state ---------------------------------------------------

fn replay_rate_and_state(name: &str, compat: TasnUpstreamCompat) {
    use akantu_core::Idx;
    use akantu_parser::param::ParameterRegistry;
    let (f, mut r) = Replay::new(&format!("ntn_contact::replay_rate_and_state::{name}"), name);
    let mut mesh = read_mesh(&input(&f, "mesh"), 2);
    let parser = materials_parser(&f);
    let _a = akantu_mesh::periodic::make_periodic(
        &mut mesh,
        SpatialDirection::X,
        Some(("slider_left", "slider_right")),
    )
    .unwrap();
    let _b = akantu_mesh::periodic::make_periodic(
        &mut mesh,
        SpatialDirection::X,
        Some(("base_left", "base_right")),
    )
    .unwrap();
    let data = parser.user_section().unwrap().clone();
    let get = |n: &str| -> Real { data.get_parameter_value(n, SearchScope::Current).unwrap() };
    let getv =
        |n: &str| -> Vec<Real> { data.get_parameter_value(n, SearchScope::Current).unwrap() };
    let tsf = get("time_step_factor");
    let mut model = model(mesh, parser);
    let mut solver =
        NtnContactSolverCallback::new(&mut model, "slider_bottom", "base_top", 1, tsf, compat)
            .unwrap();
    let shear_vel = get("shear_velocity");
    let trac_top = getv("top_traction");
    let trac_bottom = getv("bot_traction");
    let fss = get("fss");
    let (e, nu) = {
        let mat = model.material_by_name("slider").unwrap();
        (
            mat.get_param("E").unwrap().as_real().unwrap(),
            mat.get_param("nu").unwrap().as_real().unwrap(),
        )
    };
    let shear_modulus = e / (2.0 * (1.0 + nu));
    let normal_strain_applied = trac_top[1] / e - nu * nu * trac_top[1] / e;
    let (positions, slider, base): (Vec<Real>, Vec<Idx>, Vec<Idx>) = {
        let mesh_h = model.physics.base.mesh.clone();
        let m = mesh_h.read().unwrap();
        let g = m.groups();
        (
            m.nodes().as_slice().to_vec(),
            g.element_group("slider").unwrap().nodes().to_vec(),
            g.element_group("base").unwrap().nodes().to_vec(),
        )
    };
    {
        let mut disp_g = model.displacement_mut().unwrap();
        let disp = disp_g.as_mut_slice();
        for n in 0..positions.len() / 2 {
            disp[2 * n] = fss * -trac_top[1] / (shear_modulus) * positions[2 * n + 1];
            disp[2 * n + 1] = normal_strain_applied * positions[2 * n + 1];
        }
    }
    model
        .apply_neumann_bc(&FromSameDim::new(trac_top), "slider_top")
        .unwrap();
    model
        .apply_neumann_bc(&FromSameDim::new(trac_bottom), "base_bottom")
        .unwrap();
    let dt = model.time_step(None).unwrap();
    {
        let mut v_g = model.velocity_mut().unwrap();
        let v = v_g.as_mut_slice();
        for &n in &slider {
            v[n as usize * 2] = 0.5 * shear_vel;
        }
        for &n in &base {
            v[n as usize * 2] = -0.5 * shear_vel;
        }
    }
    {
        let mut inc_g = model.increment_mut().unwrap();
        let inc = inc_g.as_mut_slice();
        for &n in &slider {
            inc[n as usize * 2] = 0.5 * shear_vel * dt;
        }
        for &n in &base {
            inc[n as usize * 2] = -0.5 * shear_vel * dt;
        }
    }
    {
        let fr = solver.friction_mut();
        let phi = fr.get_param("friction_state").unwrap().as_real().unwrap();
        for n in 0..fr.is_sticking().len() {
            fr.is_sticking_mut().set_at(n, false);
            fr.update_friction_state(n, phi).unwrap();
        }
    }
    let stable = model.stable_time_step().unwrap();
    model.set_time_step(stable * tsf, None).unwrap();
    r.check("time_step", &[model.time_step(None).unwrap()]);
    let nb_steps = get("nb_steps") as usize;
    let apply_velocity = |model: &mut SolidMechanicsModel| {
        apply_velocity_bc(model, "slider_top", 0.5 * shear_vel, 0);
        apply_velocity_bc(model, "base_bottom", -0.5 * shear_vel, 0);
    };
    apply_velocity(&mut model);
    let mut rec = Series::default();
    for s in 0..nb_steps {
        solver
            .solve_step(&mut model, Some("explicit_lumped"))
            .unwrap();
        let econ = solver.external_work();
        let e = [
            model.energy("kinetic").unwrap(),
            model.energy("potential").unwrap(),
            model.energy("external work new").unwrap(),
            econ[0],
            econ[1],
        ];
        rec.push("energies", &e);
        if s % 10 == 0 {
            rec.push_state("sampled/", solver.contact(), solver.friction());
        }
        apply_velocity(&mut model);
    }
    rec.compare(&mut r, "");
    r.check(
        "final/displacement",
        model.displacement().unwrap().as_slice(),
    );
    r.check("final/velocity", model.velocity().unwrap().as_slice());
    r.finish();
}

#[test]
fn oracle_rate_and_state_upstream_mode() {
    for name in cases_with_prefix("ras_") {
        replay_rate_and_state(&name, TasnUpstreamCompat::upstream());
    }
}

/// The cases upstream instantiates do not reach the UD-039 domain: the
/// corrected defaults (regularisation option honoured) give the same
/// numbers; three representative cases (one per coefficient family and
/// state law) keep the tier short.
#[test]
fn oracle_rate_and_state_corrected_mode() {
    for name in [
        "ras_dieterich_ageing",
        "ras_regularized_dieterich_slip",
        "ras_n_shaped_ageing_regularized",
    ] {
        replay_rate_and_state(name, TasnUpstreamCompat::default());
    }
}

// ---- law scenarios ----------------------------------------------------

/// The class name upstream's `printself` prints for a law (the flash-heating
/// law does not override `printself`: it prints as the rate-and-state law).
fn printed_law_class(law: akantu_ntn_contact::FrictionLaw) -> &'static str {
    match law {
        akantu_ntn_contact::FrictionLaw::RateAndStateFlashHeating => "NTNFricLawRateAndState",
        other => other.class_name(),
    }
}

/// Class names in an upstream `printself` of a friction (outer to inner).
fn printself_classes(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim();
            let w = t.split_whitespace().next()?;
            (w.starts_with("NTN") || w.starts_with("NTRF"))
                .then(|| w.trim_end_matches('[').to_owned())
        })
        .collect()
}

fn replay_law(name: &str, compat: TasnUpstreamCompat) {
    let (f, mut r) = Replay::new(&format!("ntn_contact::replay_law::{name}"), name);
    let mesh = read_mesh(&input(&f, "mesh"), 2);
    let mut model = model(mesh, materials_parser(&f));
    let mut solver =
        NtnContactSolverCallback::new(&mut model, "slider_bottom", "base_top", 1, 0.1, compat)
            .unwrap();
    // the friction allocated by the port is the one upstream allocated
    let classes = printself_classes(param_str(&f, "friction_printself"));
    let fr = solver.friction();
    assert!(
        classes.iter().any(|c| c == printed_law_class(fr.law())),
        "{name}: {classes:?}"
    );
    assert!(
        classes
            .iter()
            .any(|c| c == fr.regularisation().class_name()),
        "{name}: {classes:?} vs {}",
        fr.regularisation().class_name()
    );
    r.check("time_step", &[model.time_step(None).unwrap()]);
    fixed_bcs(&mut model, false);
    model
        .apply_neumann_bc(
            &FromSameDim::new(vec![0.0, param_f64(&f, "normal_traction")]),
            "slider_top",
        )
        .unwrap();
    let nb_steps = param_f64(&f, "nb_steps") as usize;
    let start = param_f64(&f, "shear_start") as usize;
    let inc = param_f64(&f, "shear_increment");
    let mut rec = Series::default();
    let mut disp10: Vec<f64> = Vec::new();
    for s in 0..nb_steps {
        if s >= start {
            model
                .apply_neumann_bc(&FromSameDim::new(vec![inc, 0.0]), "slider_top")
                .unwrap();
        }
        solver.solve_step(&mut model, None).unwrap();
        rec.push_state("", solver.contact(), solver.friction());
        rec.push("external_work", &solver.external_work());
        if s % 10 == 0 {
            disp10.extend_from_slice(model.displacement().unwrap().as_slice());
        }
    }
    rec.compare(&mut r, "");
    r.check("displacement_every_10", &disp10);
    r.finish();
}

#[test]
fn oracle_law_scenarios_upstream_mode() {
    for name in cases_with_prefix("law_") {
        replay_law(&name, TasnUpstreamCompat::upstream());
    }
}

/// Corrected defaults: identical on every case except the two upstream
/// defects they fix (UD-039 at rest: domain error; UD-397: the t* → 0 limit).
#[test]
fn oracle_law_scenarios_corrected_mode() {
    for name in cases_with_prefix("law_") {
        if name == "law_dieterich_ageing_at_rest__none" || name == "law_coulomb__spc_tstar0" {
            continue;
        }
        replay_law(&name, TasnUpstreamCompat::default());
    }
}

// ---- rigid flat, mode II ----------------------------------------------

#[test]
fn oracle_ntrf_coulomb() {
    let (f, mut r) = Replay::new("ntn_contact::oracle_ntrf_coulomb", "ntrf_coulomb");
    let mesh = read_mesh(&input(&f, "mesh"), 2);
    let mut model = model(mesh, materials_parser(&f));
    let mesh_h = model.physics.base.mesh.clone();
    let compat = TasnUpstreamCompat::upstream();
    let mut contact = {
        let m = mesh_h.read().unwrap();
        NtnBaseContact::new_ntrf(&m, 2, "contact", compat).unwrap()
    };
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model.assemble_mass_lumped().unwrap();
    contact.set_reference_point(0.0, 0.0, 0.0).unwrap();
    contact.set_normal(0.0, 1.0, 0.0).unwrap();
    {
        let m = mesh_h.read().unwrap();
        let st = ModelState::of_model(&m, &mut model).unwrap();
        contact.add_surface(&st, "slider_bottom").unwrap();
    }
    let dt = model.stable_time_step().unwrap() * 0.1;
    model.set_time_step(dt, None).unwrap();
    let friction = {
        let m = mesh_h.read().unwrap();
        let st = ModelState::of_model(&m, &mut model).unwrap();
        contact.update_impedance(&st).unwrap();
        drop(st);
        initialize_ntn_friction(&contact, m.nb_nodes(), &model.physics.base.parser).unwrap()
    };
    let classes = printself_classes(param_str(&f, "friction_printself"));
    assert_eq!(classes.first().map(String::as_str), Some("NTRFFriction"));
    assert!(!friction.is_node_to_node());
    let mut cb = NtnContactSolverCallback::from_parts(contact, friction);
    let slaves: Vec<i64> = cb
        .contact()
        .slaves()
        .as_slice()
        .iter()
        .map(|&s| i64::from(s))
        .collect();
    assert_eq!(slaves, f.load_i64("slaves").unwrap().data);
    r.check(
        "lumped_boundary_slaves",
        cb.contact().lumped_boundary_slaves().as_slice(),
    );
    r.check("impedance", cb.contact().impedance().as_slice());
    fixed_bcs(&mut model, false);
    model
        .apply_neumann_bc(&FromSameDim::new(vec![0.0, -1e6]), "slider_top")
        .unwrap();
    let mut rec = Series::default();
    for s in 0..param_f64(&f, "nb_steps") as usize {
        if s >= 40 {
            model
                .apply_neumann_bc(&FromSameDim::new(vec![4e3, 0.0]), "slider_top")
                .unwrap();
        }
        cb.solve_step(&mut model, None).unwrap();
        rec.push_state("", cb.contact(), cb.friction());
        rec.push("external_work", &cb.external_work());
    }
    rec.compare(&mut r, "");
    r.check(
        "final/displacement",
        model.displacement().unwrap().as_slice(),
    );
    r.check(
        "final/global_contact_pressure",
        cb.contact().global_contact_pressure().as_slice(),
    );
    r.check(
        "final/global_friction_traction",
        cb.friction().global_friction_traction().as_slice(),
    );
    r.finish();
}

#[test]
fn oracle_miiasym_fields() {
    let (f, mut r) = Replay::new("ntn_contact::oracle_miiasym_fields", "miiasym_fields");
    let mesh = read_mesh(&input(&f, "mesh"), 2);
    let mut model = model(mesh, materials_parser(&f));
    let mesh_h = model.physics.base.mesh.clone();
    let mut contact = {
        let m = mesh_h.read().unwrap();
        NtnBaseContact::new_miiasym(&m, 2, "contact", TasnUpstreamCompat::upstream()).unwrap()
    };
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model.assemble_mass_lumped().unwrap();
    contact.set_reference_point(0.0, 0.05, 0.0).unwrap();
    contact.set_normal(0.6, 0.8, 0.0).unwrap();
    {
        let m = mesh_h.read().unwrap();
        let st = ModelState::of_model(&m, &mut model).unwrap();
        contact.add_surface(&st, "slider_bottom").unwrap();
    }
    let dt = model.stable_time_step().unwrap() * 0.1;
    model.set_time_step(dt, None).unwrap();
    let x: Vec<Real> = mesh_h.read().unwrap().nodes().as_slice().to_vec();
    let nn = x.len() / 2;
    {
        let mut d_g = model.displacement_mut().unwrap();
        let d = d_g.as_mut_slice();
        for n in 0..nn {
            d[2 * n] = 1e-4 * x[2 * n + 1] + 2e-5 * x[2 * n];
            d[2 * n + 1] = -3e-4 * x[2 * n + 1] + 1e-5;
        }
    }
    {
        let mut v_g = model.velocity_mut().unwrap();
        let v = v_g.as_mut_slice();
        for n in 0..nn {
            v[2 * n] = 0.1 * x[2 * n + 1] + 0.01 * x[2 * n];
            v[2 * n + 1] = -0.2 * x[2 * n] - 0.05;
        }
    }
    {
        let mut a_g = model.acceleration_mut().unwrap();
        let a = a_g.as_mut_slice();
        for n in 0..nn {
            a[2 * n] = 3.0 * x[2 * n] * x[2 * n + 1];
            a[2 * n + 1] = -7.0 + x[2 * n];
        }
    }
    model.assemble_internal_forces().unwrap();
    let m = mesh_h.read().unwrap();
    let st = ModelState::of_model(&m, &mut model).unwrap();
    contact.update_impedance(&st).unwrap();
    r.check("impedance", contact.impedance().as_slice());
    r.check("normals", contact.normals().as_slice());
    let rn = contact
        .compute_relative_normal_field(&st.velocity, nn)
        .unwrap();
    let rt = contact
        .compute_relative_tangential_field(&st.velocity, nn)
        .unwrap();
    let gap = contact.compute_normal_gap(&st).unwrap();
    r.check("relative_normal_velocity", &rn);
    r.check("relative_tangential_velocity", &rt);
    r.check("normal_gap", &gap);
    contact.compute_contact_pressure(&st).unwrap();
    r.check("pressure_after_noop", contact.contact_pressure().as_slice());
    contact
        .compute_contact_pressure_in_equilibrium(&st)
        .unwrap();
    r.check(
        "pressure_in_equilibrium",
        contact.contact_pressure().as_slice(),
    );
    r.check("is_in_contact", &bools(contact.is_in_contact().as_slice()));
    contact.assemble_global_contact_pressure(&m).unwrap();
    r.check(
        "global_contact_pressure",
        contact.global_contact_pressure().as_slice(),
    );
    r.finish();
}

// ---- factory ------------------------------------------------------------

#[test]
fn oracle_factory_selection() {
    let f = case("factory_selection");
    let cases = f.manifest().parameters["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 2 * 23 * 4);
    let mesh = read_mesh(&input(&f, "mesh"), 2);
    let parser = Parser::parse_str(
        "material elastic [\n name = a\n rho = 1\n E = 1\n nu = 0\n]\n",
        "factory.dat",
    )
    .unwrap();
    let mut model = model(mesh, parser);
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model.assemble_mass_lumped().unwrap();
    let mesh_h = model.physics.base.mesh.clone();
    let m = mesh_h.read().unwrap();
    let st = ModelState::of_model(&m, &mut model).unwrap();
    let compat = TasnUpstreamCompat::upstream();
    let mut ntn = NtnBaseContact::new_ntn(&m, 2, "contact", compat).unwrap();
    ntn.add_surface_pair(&st, "slider_bottom", "base_top", 1)
        .unwrap();
    let mut ntrf = NtnBaseContact::new_ntrf(&m, 2, "contact", compat).unwrap();
    ntrf.set_normal(0.0, 1.0, 0.0).unwrap();
    ntrf.add_surface(&st, "slider_bottom").unwrap();
    for c in cases {
        let contact = if c["contact"] == "ntn" { &ntn } else { &ntrf };
        let law = c["law"].as_str().unwrap();
        let option = c["option"].as_str().unwrap();
        let classes = printself_classes(c["printself"].as_str().unwrap());
        let fr = allocate_friction(contact, m.nb_nodes(), law, option).unwrap();
        let outer = if fr.is_node_to_node() {
            "NTNFriction"
        } else {
            "NTRFFriction"
        };
        assert_eq!(
            classes.first().map(String::as_str),
            Some(outer),
            "{law} {option}"
        );
        assert!(
            classes.iter().any(|x| x == printed_law_class(fr.law())),
            "{law} {option}: {classes:?}"
        );
        assert!(
            classes
                .iter()
                .any(|x| x == fr.regularisation().class_name()),
            "{law} {option}: {classes:?} vs {}",
            fr.regularisation().class_name()
        );
    }
}

// ---- support code -------------------------------------------------------

#[test]
fn oracle_parameter_reader() {
    let f = case("parameter_reader");
    let mut r = ParameterReader::new();
    r.read_input_file(input(&f, "input")).unwrap();
    let written = std::fs::read_to_string(input(&f, "written")).unwrap();
    assert_eq!(r.write_input_string(), written);
    let p = &f.manifest().parameters;
    assert_same_finite("dt", r.get_real("dt").unwrap(), p["dt"].as_f64().unwrap());
    assert_same_finite("x", r.get_real("x").unwrap(), p["x"].as_f64().unwrap());
    assert_same_finite(
        "third",
        r.get_real("third").unwrap(),
        p["third"].as_f64().unwrap(),
    );
    assert_same_finite(
        "big",
        r.get_real("big").unwrap(),
        p["big"].as_f64().unwrap(),
    );
    assert_eq!(i64::from(r.get_int("n").unwrap()), p["n"].as_i64().unwrap());
    assert_eq!(
        i64::from(r.get_int("bad").unwrap()),
        p["bad"].as_i64().unwrap()
    );
    assert_eq!(
        i64::from(r.get_uint("u").unwrap()),
        p["u"].as_i64().unwrap()
    );
    assert_eq!(
        i64::from(r.get_int("neg").unwrap()),
        p["neg"].as_i64().unwrap()
    );
    assert_eq!(r.get_bool("flag").unwrap(), p["flag"].as_bool().unwrap());
    assert_eq!(r.get_string("name").unwrap(), p["name"].as_str().unwrap());
    assert_eq!(r.has_bool("other"), p["has_other"].as_bool().unwrap());
    assert_eq!(r.has_real("noequal"), p["has_noequal"].as_bool().unwrap());
    assert_eq!(r.has_real("empty"), p["has_empty"].as_bool().unwrap());
}

#[test]
fn oracle_bc_spring() {
    let (f, mut r) = Replay::new("ntn_contact::oracle_bc_spring", "bc_spring");
    let mesh = read_mesh(&input(&f, "mesh"), 2);
    let mut model = model(mesh, materials_parser(&f));
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let dt = model.stable_time_step().unwrap() * 0.5;
    model.set_time_step(dt, None).unwrap();
    model
        .apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::Y,
            },
            "base_bottom",
        )
        .unwrap();
    model
        .apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::X,
            },
            "base_bottom",
        )
        .unwrap();
    model
        .apply_neumann_bc(&FromSameDim::new(vec![0.0, -1e5]), "slider_left")
        .unwrap();
    let mut spring = SpringBc::new(SpatialDirection::Y, 1e9, 10.0);
    spring.base_mut().insert_sub_boundary("slider_top");
    let mut sp = Vec::new();
    for _ in 0..60 {
        model.solve_step(None).unwrap();
        let reaction = spring.apply_yourself(&mut model).unwrap();
        sp.extend_from_slice(&[reaction, spring.elongation()]);
    }
    let mut fbd = ForceBasedDirichlet::new(SpatialDirection::X, 5e3, 20.0);
    fbd.insert_sub_boundary("slider_right");
    let mut fb = Vec::new();
    for _ in 0..20 {
        model.solve_step(None).unwrap();
        fb.push(fbd.apply_yourself(&mut model).unwrap());
    }
    r.check("spring", &sp);
    r.check("force_based", &fb);
    r.check(
        "final/displacement",
        model.displacement().unwrap().as_slice(),
    );
    r.finish();
}
