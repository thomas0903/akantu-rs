// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential
// tests against fixtures/model/diffusion/* (oracle/probes/diffusion/
// diffusion_probe.cc, strict reference build, Eigen sparse solver): every
// recorded value bitwise (D-008) — heat-transfer patch tests of every element
// type (static / implicit / explicit), temperature-dependent anisotropic
// conductivity, the upstream examples on the irregular frozen meshes, and the
// two-law mixed mesh (UD-276 compat switch). The replays use the default
// (corrected) mode of every switch except the solver's UD-287 switch (the
// unpatched oracle's backward Euler keeps the rate in its predictor) and,
// for the two-law case, UD-276.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Element, GhostType, Real};
use akantu_diffusion::{DiffusionUpstreamCompat, HeatTransferModel};
use akantu_validation::Fixture;
use common::*;

const ALPHA: [Real; 4] = [0.01, 0.02, 0.03, 0.04];

/// `setLinearDOF` (sequential sums).
fn linear_field(x: &[Real]) -> Real {
    let mut t = ALPHA[0];
    for (j, xj) in x.iter().enumerate() {
        t += ALPHA[j + 1] * xj;
    }
    t
}

/// The probe's quadratic field (sequential sums).
fn quadratic_field(x: &[Real], dim: usize) -> Real {
    let px = x[0];
    let py = if dim > 1 { x[1] } else { 0.0 };
    let pz = if dim > 2 { x[2] } else { 0.0 };
    let mut t = 20.;
    t += 30. * px * px;
    t += 10. * px * py;
    t -= 5. * py;
    t += 7. * pz * pz;
    t
}

fn node_coords(model: &HeatTransferModel) -> (Vec<Real>, usize) {
    let mesh = model.physics.base.mesh.read().unwrap();
    let d = mesh.spatial_dimension();
    (mesh.nodes().as_slice().to_vec(), d)
}

/// Final state common to every case.
fn check_state(f: &Fixture, model: &mut HeatTransferModel, dynamic: bool) -> usize {
    let mut n = 0;
    n += assert_bits(f, "temperature", model.temperature().unwrap().as_slice());
    if dynamic {
        n += assert_bits(
            f,
            "temperature_rate",
            model.temperature_rate().unwrap().as_slice(),
        );
    }
    n += assert_bits(
        f,
        "internal_heat_rate",
        model.internal_heat_rate().unwrap().as_slice(),
    );
    let e = model.energy("thermal").unwrap();
    n += assert_bits(f, "energy/thermal", &[e]);
    n += check_internals(f, model, 0, "law0/");
    n += check_matrix(f, model, "K");
    n += check_matrix(f, model, "M");
    n += check_matrix(f, model, "J");
    n += check_lumped(f, model);
    n
}

/// Per-element thermal energies (single-quadrature-point types only).
fn check_element_energies(f: &Fixture, model: &mut HeatTransferModel) -> usize {
    let dim = model.spatial_dimension();
    let (types, counts) = {
        let mesh = model.physics.base.mesh.read().unwrap();
        let types = types_of(&mesh, dim);
        let counts: Vec<usize> = types
            .iter()
            .map(|&t| mesh.nb_element(t, GhostType::NotGhost))
            .collect();
        (types, counts)
    };
    let mut n = 0;
    for (ty, ne) in types.into_iter().zip(counts) {
        let key = format!("energy/thermal_by_element/{}", tname(ty));
        if !has(f, &key) {
            continue;
        }
        let e: Vec<Real> = (0..ne)
            .map(|el| {
                model
                    .energy_of_element(
                        "thermal",
                        Element::new(ty, el as akantu_core::Idx, GhostType::NotGhost),
                    )
                    .unwrap()
            })
            .collect();
        n += assert_bits(f, &key, &e);
    }
    n
}

/* ---------------------------------------------------------------------- */
/* patch tests                                                             */
/* ---------------------------------------------------------------------- */

fn replay_patch(name: &str) -> usize {
    let f = case(name);
    let analysis = pstr(&f, "analysis");
    let method = method_of(&analysis);
    let mesh = patch_mesh(&f);
    let boundary = group_nodes(&mesh);
    let mut model = oracle_model_on(&f, mesh, &pstr(&f, "model_id"));
    model.init_full(&eigen_options(method)).unwrap();
    let (x, d) = node_coords(&model);
    for &node in &boundary {
        model.blocked_dofs_mut().unwrap().as_mut_slice()[node] = true;
    }
    for &node in &boundary {
        model.temperature_mut().unwrap().as_mut_slice()[node] =
            linear_field(&x[node * d..(node + 1) * d]);
    }
    let blocked: Vec<bool> = model.blocked_dofs().unwrap().as_slice().to_vec();
    assert_eq!(blocked, bools(&f, "blocked_dofs"), "{name}: blocked");
    let mut n = assert_bits(
        &f,
        "initial_temperature",
        model.temperature().unwrap().as_slice(),
    );
    let newton = set_newton(&mut model, 3, 1e-6, "solution");
    assert_eq!(
        newton,
        param(&f, "newton").as_bool().unwrap(),
        "{name}: newton"
    );
    let mut its = Vec::new();
    let mut errs = Vec::new();
    if method == AnalysisMethod::Static {
        set_newton(&mut model, 2, 2e-4, "residual");
        model.solve_step(None).unwrap();
        newton_history(&model, &mut its, &mut errs);
    } else {
        let stable = model.stable_time_step().unwrap();
        model.set_time_step(0.8 * stable, None).unwrap();
        let stable = model.stable_time_step().unwrap();
        let dt = 0.5 * stable;
        model.set_time_step(dt, None).unwrap();
        n += assert_bits(&f, "time_step", &[stable, dt]);
        for node in 0..x.len() / d {
            model.temperature_mut().unwrap().as_mut_slice()[node] =
                linear_field(&x[node * d..(node + 1) * d]);
        }
        n += assert_bits(
            &f,
            "initial_temperature_all",
            model.temperature().unwrap().as_slice(),
        );
        let want = param(&f, "step_digests").as_array().unwrap().clone();
        let mut energies = Vec::new();
        for (step, wd) in want.iter().enumerate() {
            model.solve_step(None).unwrap();
            let got = digest_arrays(&[
                model.temperature().unwrap().as_slice(),
                model.temperature_rate().unwrap().as_slice(),
                model.internal_heat_rate().unwrap().as_slice(),
            ]);
            assert_eq!(got, wd.as_str().unwrap(), "{name}: digest of step {step}");
            if newton {
                newton_history(&model, &mut its, &mut errs);
            }
            if step % 10 == 9 {
                energies.push(model.energy("thermal").unwrap());
            }
        }
        n += assert_bits(&f, "energy/thermal_every_10", &energies);
        n += want.len();
    }
    if has(&f, "newton/nb_iterations") {
        assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{name}: iterations");
        n += assert_bits(&f, "newton/error", &errs);
    }
    n += check_state(&f, &mut model, method != AnalysisMethod::Static);
    n += check_element_energies(&f, &mut model);
    let c = match model.law(0).unwrap().get_param("conductivity").unwrap() {
        akantu_parser::ParamValue::MatrixReal(m) => m,
        other => panic!("conductivity {other:?}"),
    };
    n += assert_bits(&f, "law0/conductivity", c.as_slice());
    n
}

fn patch_family(analysis: &str) {
    let cases = cases(&format!("patch_{analysis}_"));
    assert_eq!(cases.len(), 12, "every regular element type");
    let mut total = 0;
    for c in &cases {
        total += replay_patch(c);
    }
    eprintln!(
        "patch {analysis}: {} cases, {total} values bitwise",
        cases.len()
    );
}

#[test]
fn patch_static_bitwise() {
    patch_family("static");
}

#[test]
fn patch_explicit_bitwise() {
    patch_family("explicit");
}

#[test]
fn patch_implicit_bitwise() {
    patch_family("implicit");
}

/* ---------------------------------------------------------------------- */
/* temperature-dependent anisotropic conductivity                          */
/* ---------------------------------------------------------------------- */

fn replay_nonlinear(name: &str) -> usize {
    let f = case(name);
    let dim = dim_of(&f);
    let analysis = pstr(&f, "analysis");
    let method = method_of(&analysis);
    let mesh = patch_mesh(&f);
    let boundary = group_nodes(&mesh);
    let mut model = oracle_model_on(&f, mesh, "heat_transfer");
    model.init_full(&eigen_options(method)).unwrap();
    let stable = model.stable_time_step().unwrap();
    let (x, d) = node_coords(&model);
    model.temperature_mut().unwrap().as_mut_slice().fill(20.);
    for &node in &boundary {
        model.blocked_dofs_mut().unwrap().as_mut_slice()[node] = true;
        model.temperature_mut().unwrap().as_mut_slice()[node] =
            quadratic_field(&x[node * d..(node + 1) * d], dim);
    }
    let blocked: Vec<bool> = model.blocked_dofs().unwrap().as_slice().to_vec();
    assert_eq!(blocked, bools(&f, "blocked_dofs"), "{name}: blocked");
    let mut n = assert_bits(
        &f,
        "initial_temperature",
        model.temperature().unwrap().as_slice(),
    );
    let mut dt = 0.0;
    let steps = param(&f, "steps").as_u64().unwrap() as usize;
    match method {
        AnalysisMethod::Static => {
            set_newton(&mut model, 50, 1e-11, "solution");
        }
        AnalysisMethod::ImplicitDynamic => {
            set_newton(&mut model, 50, 1e-11, "solution");
            dt = 25. * stable;
            model.set_time_step(dt, None).unwrap();
        }
        _ => {
            dt = 0.5 * stable;
            model.set_time_step(dt, None).unwrap();
        }
    }
    n += assert_bits(&f, "time_step", &[stable, dt]);
    let newton = param(&f, "newton").as_bool().unwrap();
    let want = param(&f, "step_digests").as_array().cloned();
    let mut its = Vec::new();
    let mut errs = Vec::new();
    let mut energies = Vec::new();
    for s in 0..steps {
        model.solve_step(None).unwrap();
        if let Some(want) = &want {
            let got = digest_arrays(&[
                model.temperature().unwrap().as_slice(),
                model.temperature_rate().unwrap().as_slice(),
                model.internal_heat_rate().unwrap().as_slice(),
            ]);
            assert_eq!(got, want[s].as_str().unwrap(), "{name}: digest of step {s}");
        }
        if newton {
            newton_history(&model, &mut its, &mut errs);
        }
        energies.push(model.energy("thermal").unwrap());
    }
    n += assert_bits(&f, "energy/thermal_per_step", &energies);
    if newton {
        assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{name}: iterations");
        n += assert_bits(&f, "newton/error", &errs);
    }
    n += check_state(&f, &mut model, method != AnalysisMethod::Static);
    n += check_element_energies(&f, &mut model);
    n
}

#[test]
fn nonlinear_conductivity_bitwise() {
    let cases = cases("nonlinear_");
    assert_eq!(cases.len(), 15, "5 element types x 3 analyses");
    let mut total = 0;
    for c in &cases {
        total += replay_nonlinear(c);
    }
    eprintln!("nonlinear: {} cases, {total} values bitwise", cases.len());
}

/* ---------------------------------------------------------------------- */
/* the upstream examples on the irregular frozen meshes                    */
/* ---------------------------------------------------------------------- */

fn replay_example(name: &str) -> usize {
    let f = case(name);
    let dim = dim_of(&f);
    let method = method_of(name);
    let cube = name == "cube_explicit";
    let mesh = read_mesh(&mesh_path(&f), dim);
    let mut model = oracle_model_on(&f, mesh, "heat_transfer");
    model.init_full(&eigen_options(method)).unwrap();
    let stable = model.stable_time_step().unwrap();
    let (mut dt, mut steps) = (0.0, 1usize);
    if method == AnalysisMethod::ExplicitLumpedMass {
        dt = stable * 0.8;
        steps = if cube { 10 } else { 40 };
        model.set_time_step(dt, None).unwrap();
    } else if method == AnalysisMethod::ImplicitDynamic {
        dt = stable * 50.;
        steps = 10;
        model.set_time_step(dt, None).unwrap();
    }
    let mut n = assert_bits(&f, "time_step", &[stable, dt]);
    let (x, d) = node_coords(&model);
    let length = 1.0;
    for i in 0..x.len() / d {
        let p = &x[i * d..(i + 1) * d];
        model.temperature_mut().unwrap().as_mut_slice()[i] = 100.;
        if name == "square_static" {
            let (dx, dy) = (p[0], p[1]);
            // Vector<Real> dX = {dx, dy}; dX.array() -= length / 4.; dX.norm()
            let a = dx - length / 4.;
            let b = dy - length / 4.;
            let dist = (a * a + b * b).sqrt();
            if dist < 0.1 {
                model.blocked_dofs_mut().unwrap().as_mut_slice()[i] = true;
                model.temperature_mut().unwrap().as_mut_slice()[i] = 300.;
            }
            if dx.abs() < 1e-4 || dy.abs() < 1e-4 {
                model.blocked_dofs_mut().unwrap().as_mut_slice()[i] = true;
            }
            if (dx - length).abs() < 1e-4 || (dy - length).abs() < 1e-4 {
                model.blocked_dofs_mut().unwrap().as_mut_slice()[i] = true;
            }
            continue;
        }
        let (dx, dy, dz) = if cube {
            (p[0] - length / 2., p[1] - length / 2., p[2] - length / 2.)
        } else {
            (p[0] - length / 4., p[1] - length / 4., 0.0)
        };
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();
        if dist < 0.1 {
            model.blocked_dofs_mut().unwrap().as_mut_slice()[i] = true;
            model.temperature_mut().unwrap().as_mut_slice()[i] = 300.;
        }
    }
    let blocked: Vec<bool> = model.blocked_dofs().unwrap().as_slice().to_vec();
    assert_eq!(blocked, bools(&f, "blocked_dofs"), "{name}: blocked");
    assert_eq!(
        f64_digest(model.temperature().unwrap().as_slice()),
        pstr(&f, "initial_temperature/sha256"),
        "{name}: initial temperature"
    );
    let newton = param(&f, "newton").as_bool().unwrap();
    let want = param(&f, "step_digests").as_array().cloned();
    let mut its = Vec::new();
    let mut errs = Vec::new();
    let mut energies = Vec::new();
    for s in 0..steps {
        model.solve_step(None).unwrap();
        if let Some(want) = &want {
            let got = digest_arrays(&[
                model.temperature().unwrap().as_slice(),
                model.temperature_rate().unwrap().as_slice(),
                model.internal_heat_rate().unwrap().as_slice(),
            ]);
            assert_eq!(got, want[s].as_str().unwrap(), "{name}: digest of step {s}");
        }
        if newton {
            newton_history(&model, &mut its, &mut errs);
        }
        energies.push(model.energy("thermal").unwrap());
    }
    n += assert_bits(&f, "energy/thermal_per_step", &energies);
    if newton {
        assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{name}: iterations");
        n += assert_bits(&f, "newton/error", &errs);
    }
    n += check_state(&f, &mut model, method != AnalysisMethod::Static);
    n
}

#[test]
fn example_square_static_bitwise() {
    let n = replay_example("square_static");
    eprintln!("square_static: {n} values bitwise");
}

#[test]
fn example_square_explicit_bitwise() {
    let n = replay_example("square_explicit");
    eprintln!("square_explicit: {n} values bitwise");
}

#[test]
fn example_square_implicit_bitwise() {
    let n = replay_example("square_implicit");
    eprintln!("square_implicit: {n} values bitwise");
}

#[test]
fn example_cube_explicit_bitwise() {
    let n = replay_example("cube_explicit");
    eprintln!("cube_explicit: {n} values bitwise");
}

/* ---------------------------------------------------------------------- */
/* two laws on a mixed mesh (UD-276)                                       */
/* ---------------------------------------------------------------------- */

fn two_laws_model(f: &Fixture, compat: bool) -> HeatTransferModel {
    let mesh = read_mesh(&mesh_path(f), 2);
    let mut model = oracle_model_on(f, mesh, "heat_transfer");
    model.physics.upstream_compat = DiffusionUpstreamCompat {
        empty_filter_returns: compat,
        ..DiffusionUpstreamCompat::default()
    };
    model.set_law_selector(Box::new(
        akantu_model::constitutive_law::ElementDataSelector::new("physical_names"),
    ));
    model
        .init_full(&eigen_options(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let stable = model.stable_time_step().unwrap();
    model.set_time_step(0.5 * stable, None).unwrap();
    let (x, d) = node_coords(&model);
    for nd in 0..x.len() / d {
        let (px, py) = (x[nd * d], x[nd * d + 1]);
        model.temperature_mut().unwrap().as_mut_slice()[nd] = 10. + 5. * px + 2. * py * px;
    }
    model.blocked_dofs_mut().unwrap().as_mut_slice()[0] = true;
    model.blocked_dofs_mut().unwrap().as_mut_slice()[4] = true;
    for _ in 0..5 {
        model.solve_step(None).unwrap();
    }
    if !model.dof_manager().has_matrix("K") {
        model
            .dof_manager_mut()
            .new_matrix("K", akantu_core::enums::MatrixType::Symmetric)
            .unwrap();
    }
    model.assemble_diffusivity_matrix().unwrap();
    model
}

#[test]
fn two_laws_mixed_upstream_mode_bitwise() {
    let f = case("two_laws_mixed");
    let mut model = two_laws_model(&f, true);
    let mut n = 0;
    n += assert_bits(&f, "temperature", model.temperature().unwrap().as_slice());
    n += assert_bits(
        &f,
        "temperature_rate",
        model.temperature_rate().unwrap().as_slice(),
    );
    n += assert_bits(
        &f,
        "internal_heat_rate",
        model.internal_heat_rate().unwrap().as_slice(),
    );
    n += check_lumped(&f, &model);
    n += check_matrix(&f, &model, "K");
    for l in 0..2 {
        n += check_internals(&f, &model, l, &format!("law{l}/"));
    }
    // upstream integrates without the filter and asserts (UD-278): the
    // oracle recorded the exception; the port computes the energy.
    assert!(
        param(&f, "energy/thermal_exception").is_string()
            || param(&f, "energy/thermal").is_number()
    );
    let e = model.energy("thermal").unwrap();
    assert!(e.is_finite() && e > 0.0);
    eprintln!("two_laws_mixed (UD-276 compat): {n} values bitwise");
}

#[test]
fn two_laws_mixed_default_mode_assembles_every_law() {
    let f = case("two_laws_mixed");
    let upstream = two_laws_model(&f, true);
    let fixed = two_laws_model(&f, false);
    // default: both laws assembled; the quadrangle law contributes flow
    let k_up = upstream.dof_manager().matrix("K").unwrap().nnz();
    let k_fix = fixed.dof_manager().matrix("K").unwrap().nnz();
    assert!(
        k_fix > k_up,
        "the quadrangle law adds K entries ({k_fix} vs {k_up})"
    );
    let fi_g = fixed.internal_heat_rate().unwrap();
    let fi = fi_g.as_slice();
    let sum: Real = fi.iter().sum();
    // conservation: the internal flow of a conductor sums to zero
    let scale = fi.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(sum.abs() <= 1e-12 * scale.max(1.0), "Σ q_int = {sum:e}");
    // the default-mode internal flow by
    // value — every law's elements contribute, q_int = −K T (the linear
    // law; after the last explicit step the flow is that of the current
    // temperature: forward Euler's corrector does not move T) to round-off
    let t_g = fixed.temperature().unwrap();
    let t = t_g.as_slice();
    let eq = fixed
        .dof_manager()
        .local_equation_numbers("temperature")
        .unwrap()
        .to_vec();
    let mut node_of = vec![usize::MAX; eq.len()];
    for (node, &e) in eq.iter().enumerate() {
        node_of[e as usize] = node;
    }
    let k = fixed.dof_manager().matrix("K").unwrap();
    let (rows, cols, vals) = k.coo();
    let mut kt = vec![0.0; t.len()];
    for ((&i, &j), &v) in rows.iter().zip(cols).zip(vals) {
        let (ni, nj) = (node_of[i as usize], node_of[j as usize]);
        kt[ni] += v * t[nj];
        if i != j {
            // symmetric storage: one triangle
            kt[nj] += v * t[ni];
        }
    }
    let kscale = kt.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    assert!(kscale > 0.0);
    for (n, (q, ktn)) in fi.iter().zip(&kt).enumerate() {
        assert!(
            (q + ktn).abs() <= 1e-12 * kscale,
            "node {n}: q_int = {q:e}, −(K T) = {:e}",
            -ktn
        );
    }
}
