// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Replay of the
// adversarial oracle cases fixtures/model/diffusion/v102_* (probe
// oracle/probes/diffusion/diffusion_v102_probe.cc, driving upstream's
// src/model/diffusion_model/* unmodified) with the port, and the UD-287
// convergence analysis, written independently of the port: fixtures read
// from the committed corpus, values
// compared under the D-046 exact rule (`akantu_validation::exact_eq`), and
// the UD-283 case adapted to UD-811 (the port refuses cohesive elements at
// `init_full`, as upstream).

mod common;

use akantu_core::enums::EventPriority;
use akantu_core::enums::{
    AnalysisMethod, IntegrationSchemeType, MatrixType, NonLinearSolverType, SparseSolverType,
    TimeStepSolverType,
};
use akantu_core::{ElementType, GhostType, Real};
use akantu_diffusion::{DiffusionUpstreamCompat, HeatTransferModel};
use akantu_mesh::{Mesh, MeshEventHandler, MeshPart, SharedMesh};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::{ModelSolverOptions, SolutionType};
use akantu_validation::Fixture;
use common::*;

fn open(id: &str) -> Fixture {
    case(&format!("v102_{id}"))
}

/// Exact comparison under D-046 (identical bits, or two default quiet NaNs
/// of either sign: the x86_64 oracle's NaNs against an aarch64 port run).
fn bits(case: &str, what: &str, got: &[Real], want: &[Real]) -> usize {
    assert_eq!(got.len(), want.len(), "{case}: {what}: length");
    let bad = |(a, b): &(&Real, &Real)| !akantu_validation::exact_eq(**b, **a);
    let n = got.iter().zip(want).filter(bad).count();
    if n != 0 {
        let (i, (a, b)) = got
            .iter()
            .zip(want)
            .enumerate()
            .find(|(_, p)| bad(p))
            .unwrap();
        panic!(
            "{case}: {what}: {n} of {} differ; first [{i}] port {a:e} oracle {b:e}",
            want.len()
        );
    }
    want.len()
}

fn upstream_solver_compat(m: &mut HeatTransferModel) {
    m.dof_manager_mut()
        .set_upstream_compat(akantu_solver::UpstreamCompat {
            generalized_trapezoidal_keeps_rate_in_predictor: true,
            ..akantu_solver::UpstreamCompat::default()
        });
}

fn set_newton_of(m: &mut HeatTransferModel, id: Option<&str>, max: i64, thr: Real, crit: &str) {
    let nls = m.solver.non_linear_solver_mut(id).unwrap();
    if nls.as_newton_raphson_mut().is_none() {
        return;
    }
    nls.set_param("max_iterations", ParamValue::Int(max))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(thr)).unwrap();
    nls.set_param("convergence_type", ParamValue::Enum(crit.into()))
        .unwrap();
}

/* ---------------------------------------------------------------------- */
/* UD-287 bar                                                              */
/* ---------------------------------------------------------------------- */

struct BarOut {
    t: Vec<Real>,
    iterations: i64,
}

/// The probe's `bar_run` with the port. `keep_rate` = upstream predictor.
fn bar_port(
    f: &Fixture,
    n: usize,
    steps: usize,
    scheme: &str,
    driver: &str,
    t0: &[Real],
    keep_rate: bool,
) -> BarOut {
    let mesh = read_mesh(&f.input_path(&format!("mesh_N{n}")).unwrap(), 1);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)
            .unwrap();
    // as the probe: static default solver, one dynamic solver under test
    m.init_full(&eigen_options(AnalysisMethod::Static)).unwrap();
    let o = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Dynamic),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    let sid = "under_test";
    m.get_new_solver(sid, &o).unwrap();
    let alpha = if scheme == "be" { 1.0 } else { 0.5 };
    m.set_integration_scheme(
        sid,
        "temperature",
        if scheme == "be" {
            IntegrationSchemeType::BackwardEuler
        } else {
            IntegrationSchemeType::TrapezoidalRule1
        },
        if driver == "rate" {
            SolutionType::TEMPERATURE_RATE
        } else {
            SolutionType::TEMPERATURE
        },
    )
    .unwrap();
    set_newton_of(&mut m, Some(sid), 10, 1e-10, "solution");
    m.dof_manager_mut()
        .set_upstream_compat(akantu_solver::UpstreamCompat {
            generalized_trapezoidal_keeps_rate_in_predictor: keep_rate,
            ..akantu_solver::UpstreamCompat::default()
        });
    let dt = 0.1 / steps as Real;
    m.set_time_step(dt, Some(sid)).unwrap();
    m.temperature_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(t0);
    {
        let mut b_g = m.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        b[0] = true;
        b[n] = true;
    }
    let mut iterations = 0i64;
    for _ in 0..steps {
        if driver == "shift" {
            let rate = m.diffusion_rate().unwrap().as_slice().to_vec();
            let blocked = m.blocked_dofs().unwrap().as_slice().to_vec();
            let mut t_g = m.temperature_mut().unwrap();
            let t = t_g.as_mut_slice();
            for i in 0..=n {
                if !blocked[i] {
                    t[i] += alpha * dt * rate[i];
                }
            }
        }
        m.solve_step(Some(sid)).unwrap();
        iterations += m
            .solver
            .non_linear_solver(Some(sid))
            .unwrap()
            .nb_iterations() as i64;
    }
    BarOut {
        t: m.temperature().unwrap().as_slice().to_vec(),
        iterations,
    }
}

/// Port default (corrected) trapezoidal rule 1 with initial rate `rate0`.
fn bar_port_rate(f: &Fixture, n: usize, steps: usize, t0: &[Real], rate0: &[Real]) -> Vec<Real> {
    let mesh = read_mesh(&f.input_path(&format!("mesh_N{n}")).unwrap(), 1);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)
            .unwrap();
    m.init_full(&eigen_options(AnalysisMethod::Static)).unwrap();
    let o = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Dynamic),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    m.get_new_solver("cn", &o).unwrap();
    m.set_integration_scheme(
        "cn",
        "temperature",
        IntegrationSchemeType::TrapezoidalRule1,
        SolutionType::TEMPERATURE,
    )
    .unwrap();
    set_newton_of(&mut m, Some("cn"), 10, 1e-10, "solution");
    m.set_time_step(0.1 / steps as Real, Some("cn")).unwrap();
    m.temperature_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(t0);
    m.diffusion_rate_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(rate0);
    {
        let mut b_g = m.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        b[0] = true;
        b[n] = true;
    }
    for _ in 0..steps {
        m.solve_step(Some("cn")).unwrap();
    }
    m.temperature().unwrap().as_slice().to_vec()
}

/// Discrete sine transform coefficients of `u` (interior nodes 1..n-1).
fn dst(u: &[Real], n: usize) -> Vec<Real> {
    (1..n)
        .map(|j| {
            let mut s = 0.0;
            for (i, ui) in u.iter().enumerate().take(n).skip(1) {
                s += ui * (std::f64::consts::PI * (j * i) as Real / n as Real).sin();
            }
            2.0 / n as Real * s
        })
        .collect()
}

/// Semi-discrete exact solution exp(-M^-1 K t) u0 on the uniform bar,
/// spectrally (the discrete sines are the eigenvectors of K and M).
fn semi_discrete_spectral(u0: &[Real], n: usize, t: Real) -> Vec<Real> {
    let h = 1.0 / n as Real;
    let c = dst(u0, n);
    (0..=n)
        .map(|i| {
            if i == 0 || i == n {
                return 0.0;
            }
            let mut s = 0.0;
            for (jj, cj) in c.iter().enumerate() {
                let j = jj + 1;
                let th = std::f64::consts::PI * j as Real * h;
                let lam = 6.0 / (h * h) * (1.0 - th.cos()) / (2.0 + th.cos());
                s += cj * (-lam * t).exp() * (std::f64::consts::PI * (j * i) as Real * h).sin();
            }
            s
        })
        .collect()
}

/// The same with a dense matrix exponential (scaling and squaring, Taylor
/// degree 18) of A = -M^-1 K assembled independently (linear elements).
fn semi_discrete_expm(u0: &[Real], n: usize, t: Real) -> Vec<Real> {
    let m = n - 1;
    let h = 1.0 / n as Real;
    // K = (1/h) tridiag(-1, 2, -1), M = (h/6) tridiag(1, 4, 1) on interior
    let mut kk = vec![0.0; m * m];
    let mut mm = vec![0.0; m * m];
    for i in 0..m {
        kk[i * m + i] = 2.0 / h;
        mm[i * m + i] = 4.0 * h / 6.0;
        if i + 1 < m {
            kk[i * m + i + 1] = -1.0 / h;
            kk[(i + 1) * m + i] = -1.0 / h;
            mm[i * m + i + 1] = h / 6.0;
            mm[(i + 1) * m + i] = h / 6.0;
        }
    }
    // A = -M^-1 K by Gauss-Jordan on [M | K]
    let mut aug = vec![0.0; m * 2 * m];
    for i in 0..m {
        for j in 0..m {
            aug[i * 2 * m + j] = mm[i * m + j];
            aug[i * 2 * m + m + j] = kk[i * m + j];
        }
    }
    for p in 0..m {
        let piv = aug[p * 2 * m + p];
        for j in 0..2 * m {
            aug[p * 2 * m + j] /= piv;
        }
        for r in 0..m {
            if r != p {
                let fct = aug[r * 2 * m + p];
                if fct != 0.0 {
                    for j in 0..2 * m {
                        aug[r * 2 * m + j] -= fct * aug[p * 2 * m + j];
                    }
                }
            }
        }
    }
    let mut a = vec![0.0; m * m];
    for i in 0..m {
        for j in 0..m {
            a[i * m + j] = -aug[i * 2 * m + m + j] * t;
        }
    }
    let norm = a
        .chunks(m)
        .map(|r| r.iter().map(|x| x.abs()).sum::<Real>())
        .fold(0.0, Real::max);
    let mut s = 0;
    while norm / (1u64 << s) as Real > 0.5 {
        s += 1;
    }
    let scale = (1u64 << s) as Real;
    for x in &mut a {
        *x /= scale;
    }
    let matmul = |x: &[Real], y: &[Real]| -> Vec<Real> {
        let mut z = vec![0.0; m * m];
        for i in 0..m {
            for k in 0..m {
                let xik = x[i * m + k];
                if xik != 0.0 {
                    for j in 0..m {
                        z[i * m + j] += xik * y[k * m + j];
                    }
                }
            }
        }
        z
    };
    let mut e = vec![0.0; m * m];
    let mut term = vec![0.0; m * m];
    for i in 0..m {
        e[i * m + i] = 1.0;
        term[i * m + i] = 1.0;
    }
    for k in 1..=18 {
        term = matmul(&term, &a);
        for x in &mut term {
            *x /= k as Real;
        }
        for (ei, ti) in e.iter_mut().zip(&term) {
            *ei += ti;
        }
    }
    for _ in 0..s {
        e = matmul(&e, &e);
    }
    let mut out = vec![0.0; n + 1];
    for i in 0..m {
        let mut v = 0.0;
        for j in 0..m {
            v += e[i * m + j] * u0[j + 1];
        }
        out[i + 1] = v;
    }
    out
}

/// Continuum solution at the nodes (k = 1, rho c = 1).
fn continuum(ic: &str, n: usize, t: Real) -> Vec<Real> {
    let pi = std::f64::consts::PI;
    (0..=n)
        .map(|i| {
            let x = i as Real / n as Real;
            match ic {
                "sine" => (-pi * pi * t).exp() * (pi * x).sin(),
                _ => {
                    let mut s = 0.0;
                    let mut k = 1;
                    while k < 400 {
                        let kf = k as Real;
                        s += 32.0 / (kf * kf * kf * pi * pi * pi)
                            * (-kf * kf * pi * pi * t).exp()
                            * (kf * pi * x).sin();
                        k += 2;
                    }
                    s
                }
            }
        })
        .collect()
}

fn maxdiff(a: &[Real], b: &[Real]) -> Real {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, Real::max)
}

#[test]
fn v102_ud287_bar() {
    let f = open("ud287_bar");
    let runs = param(&f, "runs").as_array().unwrap().clone();
    let mut table = String::from(
        "scheme\tdriver\tic\tN\tS\tsource\terr_semidiscrete\terr_continuum\titerations\n",
    );
    let mut nvals = 0;
    let mut corrected_vs_shift: Real = 0.0;
    for r in &runs {
        let key = r["key"].as_str().unwrap();
        let n = r["N"].as_u64().unwrap() as usize;
        let s = r["S"].as_u64().unwrap() as usize;
        let scheme = r["scheme"].as_str().unwrap();
        let driver = r["driver"].as_str().unwrap();
        let ic = r["ic"].as_str().unwrap();
        assert!(r.get("exception").is_none(), "{key}: oracle exception");
        let t0 = f64s(&f, r["T0"].as_str().unwrap());
        let oracle = f64s(&f, &format!("{key}/T"));
        // 1. bitwise replay of the unmodified oracle (UD-287 compat)
        let up = bar_port(&f, n, s, scheme, driver, &t0, true);
        nvals += bits(key, "T (compat)", &up.t, &oracle);
        assert_eq!(
            up.iterations,
            r["iterations"].as_i64().unwrap(),
            "{key}: Newton iterations"
        );
        let sd = semi_discrete_spectral(&t0, n, 0.1);
        if n <= 32 {
            let ex = semi_discrete_expm(&t0, n, 0.1);
            let d = maxdiff(&sd, &ex);
            assert!(d < 1e-12, "{key}: spectral vs expm {d:e}");
        }
        let co = continuum(ic, n, 0.1);
        table.push_str(&format!(
            "{scheme}\t{driver}\t{ic}\t{n}\t{s}\toracle\t{:.6e}\t{:.6e}\t{}\n",
            maxdiff(&oracle, &sd),
            maxdiff(&oracle, &co),
            up.iterations
        ));
        // 2. the port's corrected default (plain driver only)
        if driver == "plain" {
            let cor = bar_port(&f, n, s, scheme, driver, &t0, false);
            table.push_str(&format!(
                "{scheme}\tplain\t{ic}\t{n}\t{s}\tport-default\t{:.6e}\t{:.6e}\t{}\n",
                maxdiff(&cor.t, &sd),
                maxdiff(&cor.t, &co),
                cor.iterations
            ));
            // the oracle's shift driver is the textbook scheme on the
            // unmodified oracle: the port's corrected default must agree
            let shift_key = key.replace("/plain/", "/shift/");
            if let Ok(q) = f.load_f64(&format!("{shift_key}/T")) {
                let d = maxdiff(&cor.t, &q.data);
                corrected_vs_shift = corrected_vs_shift.max(d);
            }
        }
    }
    println!("{table}");
    println!("max |port default - oracle shift| = {corrected_vs_shift:e}");
    // corrected trapezoidal rule 1 from a CONSISTENT initial rate
    // (M T_dot0 = -K T0, spectrally), time refinement at N = 16: order 2
    // (from the default T_dot0 = 0 the table above shows order 1)
    for ic in ["sine", "poly"] {
        let n = 16;
        let t0 = f64s(&f, &format!("T0/{ic}/N{n}"));
        let h = 1.0 / n as Real;
        let c = dst(&t0, n);
        let rate0: Vec<Real> = (0..=n)
            .map(|i| {
                if i == 0 || i == n {
                    return 0.0;
                }
                let mut s = 0.0;
                for (jj, cj) in c.iter().enumerate() {
                    let j = jj + 1;
                    let th = std::f64::consts::PI * j as Real * h;
                    let lam = 6.0 / (h * h) * (1.0 - th.cos()) / (2.0 + th.cos());
                    s -= cj * lam * (std::f64::consts::PI * (j * i) as Real * h).sin();
                }
                s
            })
            .collect();
        let sd = semi_discrete_spectral(&t0, n, 0.1);
        let mut errs = Vec::new();
        for steps in [10usize, 20, 40, 80, 160] {
            let out = bar_port_rate(&f, n, steps, &t0, &rate0);
            errs.push(maxdiff(&out, &sd));
        }
        let orders: Vec<Real> = errs.windows(2).map(|w| (w[0] / w[1]).log2()).collect();
        println!("cn corrected, consistent T_dot0, {ic}: errors {errs:?} orders {orders:.3?}");
        for o in &orders {
            assert!((o - 2.0).abs() < 0.1, "{ic}: order {o}");
        }
    }
    assert!(corrected_vs_shift < 1e-13, "{corrected_vs_shift:e}");
    println!("ud287_bar: {} runs, {nvals} values bitwise", runs.len());
}

/* ---------------------------------------------------------------------- */
/* generic cases                                                           */
/* ---------------------------------------------------------------------- */

fn replay_case(id: &str, compat: DiffusionUpstreamCompat, consistent_first: bool) -> usize {
    let f = open(id);
    let dim = param(&f, "spatial_dimension").as_u64().unwrap() as usize;
    let analysis = pstr(&f, "analysis");
    let method = method_of(&analysis);
    let steps = param(&f, "steps").as_u64().unwrap() as usize;
    let oracle_exc = param(&f, "exception").as_str().map(str::to_owned);
    let stages: Vec<String> = param(&f, "stages")
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect();
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)
            .unwrap();
    m.physics.upstream_compat = compat;
    upstream_solver_compat(&mut m);
    let mut n = 0;
    let mut port_stage = String::from("initFull");
    let r: akantu_core::Result<()> = (|| {
        m.init_full(&eigen_options(method))?;
        port_stage = "getStableTimeStep".into();
        let stable = m.stable_time_step()?;
        let ts = f64s(&f, "time_step");
        n += bits(id, "stable time step", &[stable], &ts[..1]);
        if method != AnalysisMethod::Static {
            m.set_time_step(ts[1], None)?;
        }
        set_newton_of(&mut m, None, 30, 1e-10, "solution");
        m.temperature_mut()?
            .as_mut_slice()
            .copy_from_slice(&f64s(&f, "initial_temperature"));
        let b = bools(&f, "blocked_dofs");
        m.blocked_dofs_mut()?.as_mut_slice().copy_from_slice(&b);
        if has(&f, "initial_rate") {
            m.diffusion_rate_mut()?
                .as_mut_slice()
                .copy_from_slice(&f64s(&f, "initial_rate"));
        }
        if consistent_first {
            port_stage = "assembleCapacityMatrix".into();
            if !m.dof_manager().has_matrix("M") {
                m.dof_manager_mut().new_matrix("M", MatrixType::Symmetric)?;
            }
            m.assemble_capacity_matrix()?;
        }
        let mut its = Vec::new();
        let mut errs = Vec::new();
        let mut en = Vec::new();
        let newton = m
            .solver
            .non_linear_solver_mut(None)?
            .as_newton_raphson_mut()
            .is_some();
        for s in 0..steps {
            port_stage = format!("solveStep {s}");
            m.solve_step(None)?;
            if newton {
                newton_history(&m, &mut its, &mut errs);
            }
            en.push(m.energy("thermal")?);
        }
        port_stage = "record".into();
        n += bits(
            id,
            "temperature",
            m.temperature()?.as_slice(),
            &f64s(&f, "temperature"),
        );
        if method != AnalysisMethod::Static {
            n += bits(
                id,
                "temperature_rate",
                m.temperature_rate()?.as_slice(),
                &f64s(&f, "temperature_rate"),
            );
        }
        n += bits(
            id,
            "internal_heat_rate",
            m.internal_heat_rate()?.as_slice(),
            &f64s(&f, "internal_heat_rate"),
        );
        n += bits(id, "energies", &en, &f64s(&f, "energy/thermal_per_step"));
        if newton {
            assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{id}: iterations");
            n += bits(id, "newton error", &errs, &f64s(&f, "newton/error"));
        }
        for mid in ["K", "M"] {
            if param(&f, &format!("{mid}/exists")).as_bool() == Some(true) {
                let mat = m.dof_manager().matrix(mid)?;
                let (r, c, v) = mat.coo();
                assert_eq!(
                    r.to_vec(),
                    i64s(&f, &format!("{mid}/rows")),
                    "{id}: {mid} rows"
                );
                assert_eq!(
                    c.to_vec(),
                    i64s(&f, &format!("{mid}/cols")),
                    "{id}: {mid} cols"
                );
                n += bits(id, mid, v, &f64s(&f, &format!("{mid}/values")));
            }
        }
        if param(&f, "lumped_M/exists").as_bool() == Some(true) {
            let lm = m.lumped_capacity()?;
            n += bits(id, "lumped M", lm.as_slice(), &f64s(&f, "lumped_M"));
        }
        port_stage = "done".into();
        Ok(())
    })();
    match (&r, &oracle_exc) {
        (Ok(()), None) => {}
        (Err(e), Some(o)) => {
            println!(
                "{id}: both fail: oracle at {:?}: {o}; port at {port_stage}: {e}",
                stages.last()
            );
            assert_eq!(Some(&port_stage), stages.last(), "{id}: failure stage");
        }
        (Err(e), None) => panic!("{id}: port fails at {port_stage}: {e}; oracle succeeds"),
        (Ok(()), Some(o)) => panic!(
            "{id}: oracle fails ({o}) at {:?}, port succeeds",
            stages.last()
        ),
    }
    println!("{id}: {n} values bitwise");
    n
}

#[test]
fn v102_aniso2d() {
    for a in ["static", "explicit", "implicit"] {
        replay_case(
            &format!("aniso2d_{a}"),
            DiffusionUpstreamCompat::default(),
            false,
        );
    }
}

#[test]
fn v102_aniso3d() {
    for a in ["explicit", "implicit"] {
        replay_case(
            &format!("aniso3d_{a}"),
            DiffusionUpstreamCompat::default(),
            false,
        );
    }
}

#[test]
fn v102_rate0() {
    for a in ["explicit", "implicit"] {
        replay_case(
            &format!("rate0_{a}"),
            DiffusionUpstreamCompat::default(),
            false,
        );
    }
}

#[test]
fn v102_kink() {
    for a in ["explicit", "implicit"] {
        replay_case(
            &format!("kink_{a}"),
            DiffusionUpstreamCompat::default(),
            false,
        );
    }
}

#[test]
fn v102_zerorho() {
    for a in ["static", "implicit", "explicit"] {
        replay_case(
            &format!("zerorho_{a}"),
            DiffusionUpstreamCompat::default(),
            false,
        );
    }
}

#[test]
fn v102_ud281() {
    let compat = DiffusionUpstreamCompat {
        lumped_capacity_gated_by_consistent_flag: true,
        ..DiffusionUpstreamCompat::default()
    };
    replay_case("ud281_lumped", compat, true);
}

/// `SharedMesh::mutate` with the model as the mesh's only consumer (its
/// token moved out for the call, as in the SMM regressions).
fn mutate(
    m: &mut HeatTransferModel,
    change: impl FnOnce(&mut akantu_mesh::MeshTransaction<'_>) -> akantu_core::Result<()>,
) -> akantu_core::Result<()> {
    let shared = m.physics.base.mesh.clone();
    let scratch = SharedMesh::new(Mesh::new(m.spatial_dimension(), "scratch").unwrap());
    let placeholder = scratch
        .register_consumer("placeholder", EventPriority::MODEL, MeshPart::Main)
        .unwrap();
    let token = std::mem::replace(&mut m.0.token, placeholder);
    let r = shared
        .mutate(
            &mut [(&token, m as &mut dyn MeshEventHandler)],
            change,
            None,
        )
        .map(|_| ());
    m.0.token = token;
    r
}

/// UD-283 / UD-811: oracle Δt before / after a `_cohesive_2d_4` appended
/// with `MeshAccessor` after `initFull` (0.142802 → 0). The port: Δt before
/// bitwise; the cohesive element appended without an event (its analogue of
/// raw mesh access) leaves the default Δt unchanged (regular kinds) and the
/// compat switch gives the oracle's 0 bitwise.
#[test]
fn v102_ud283() {
    let f = open("ud283_cohesive_dt");
    let ts = f64s(&f, "stable_time_step");
    println!("oracle: before {:e}, after {:e}", ts[0], ts[1]);
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)
            .unwrap();
    m.init_full(&eigen_options(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    bits(
        "ud283",
        "dt before",
        &[m.stable_time_step().unwrap()],
        &ts[..1],
    );
    mutate(&mut m, |tx| {
        tx.push_element(
            MeshPart::Main,
            ElementType::Cohesive2d4,
            GhostType::NotGhost,
            &[0, 1, 3, 2],
        )
        .map(|_| ())
    })
    .unwrap();
    let d = m.stable_time_step().unwrap();
    bits("ud283", "dt default (regular kinds)", &[d], &ts[..1]);
    m.physics
        .upstream_compat
        .stable_time_step_includes_all_kinds = true;
    let d = m.stable_time_step().unwrap();
    bits("ud283", "dt compat (all kinds)", &[d], &ts[1..2]);
}

/// UD-811: upstream's `initFull` on a mesh holding a cohesive element
/// throws at law assignment (recorded by the oracle: stage `initFull`); the
/// port's `init_full` refuses likewise (missing element type block).
#[test]
fn v102_ud283_cohesive_init() {
    let f = open("ud283_cohesive_init");
    let stages = param(&f, "stages");
    let exc = param(&f, "exception").as_str().unwrap_or("").to_owned();
    println!("oracle: stages {stages} exception {exc}");
    assert_eq!(
        stages
            .as_array()
            .and_then(|a| a.last())
            .and_then(|s| s.as_str()),
        Some("initFull"),
        "the oracle fails at initFull"
    );
    assert!(exc.contains("_cohesive_2d_4"), "{exc}");
    let mut mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    akantu_mesh::MeshAccessor::new(&mut mesh)
        .connectivity_mut(ElementType::Cohesive2d4, GhostType::NotGhost)
        .push_tuple(&[0, 1, 3, 2]);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut m =
        HeatTransferModel::with_parser(SharedMesh::new(mesh), None, "heat_transfer", parser)
            .unwrap();
    let r = m.init_full(&eigen_options(AnalysisMethod::ExplicitLumpedMass));
    assert!(
        matches!(r, Err(akantu_core::Error::NotFound { .. })),
        "the port refuses at init_full: {r:?}"
    );
}
