// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/scalability_test (port of
// examples/c++/scalability_test/cohesive_extrinsic.cc, upstream ctest
// `scalability_test-test`).

mod examples_cohesive_common;
mod examples_common;
#[path = "../examples/scalability_test/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod scalability_test;

use akantu::cohesive::SolidMechanicsModelCohesive;
use akantu::cohesive::material::COHESIVE_FE_ENGINE;
use akantu::core::array::ArrayView;
use akantu::core::{ElementType, GhostType, Idx, Real};
use akantu::model::constitutive_law::ConstitutiveLaw;
use examples_cohesive_common::{assert_oracle_status, probe_input};
use examples_common::oracle::{assert_csv_values_bitwise, oracle};
use examples_common::scratch;

/// `σc`, `δc` and `β` of the `insertion` law (`cohesive_linear`) of
/// upstream's `material-cohesive.dat` and of the variant's
/// `scalability_cohesive_full.dat` (checked by
/// [`scalability_test_cohesive_law_parameters`]).
const SIGMA_C: Real = 70e6;
const DELTA_C: Real = 0.217e-6;
const BETA: Real = 3.0;

/// Timing lines of the upstream example (`Chrono` and the final times).
fn timing(line: &str) -> bool {
    line.starts_with("Passing the first ")
        || line.starts_with("Full time: ")
        || line.starts_with("Init time: ")
        || line.starts_with("Step time: ")
}

/// The run of `input` (an input file of the example or of the oracle
/// variants) limited to `steps`, with upstream's cohesive defects
/// (`--upstream-compat`) or the corrected defaults.
fn run_with(
    input: &std::path::Path,
    name: &str,
    steps: Option<usize>,
    upstream_compat: bool,
) -> scalability_test::Outcome {
    let out = scratch(name);
    let mut ex = scalability_test::example().for_test(&out);
    ex.input_file = input.to_path_buf();
    ex.steps = steps;
    ex.upstream_compat = upstream_compat;
    scalability_test::run(&ex).unwrap()
}

/// [`run_with`] reproducing upstream (`--upstream-compat`: UD-217 tangents,
/// the comparisons with the oracle).
fn run(input: &std::path::Path, name: &str, steps: Option<usize>) -> scalability_test::Outcome {
    run_with(input, name, steps, true)
}

/// Energies of `r` (step 0 and every step) bitwise against the 17-digit
/// `energies.csv` of the oracle variant case `case`, and the number of
/// cohesive elements after every step against its `cohesive_full.csv`.
fn assert_energies(case: &str, r: &scalability_test::Outcome) {
    let o = oracle(case);
    let lines = o.kept_lines("energies.csv");
    assert_eq!(lines[0], "step, ed, ep, ek, ew, et");
    assert!(r.energies.len() < lines.len(), "{case}: oracle records");
    for (s, (e, l)) in r.energies.iter().zip(&lines[1..]).enumerate() {
        let c: Vec<&str> = l.split(", ").collect();
        assert_eq!(c[0], s.to_string(), "{case}: step column");
        assert_csv_values_bitwise(&format!("{case}: energies of step {s}"), e, &c[1..]);
    }
    // without insertion (`cohesive_insertion = false`) the count stays 0
    let counts = o.kept_lines("cohesive_full.csv");
    assert_eq!(counts[0], "step, nb_cohesive_element");
    let steps = r.energies.len() - 1;
    let mut nb = 0;
    for (s, l) in counts[1..=steps].iter().enumerate() {
        nb += r.inserted.get(s).copied().unwrap_or(0);
        assert_eq!(l, &format!("{}, {nb}", s + 1), "{case}: cohesive elements");
    }
}

/// The energies of the cohesive law `insertion` at the end of a run
/// (UD-803).
#[derive(Debug)]
struct Dissipation {
    /// Upstream's `getEnergy("dissipated")`: `∫ (total − reversible)` of
    /// the trapezoidal bookkeeping of `MaterialCohesive::updateEnergies`.
    upstream: Real,
    /// The law's exact dissipation `∫ ½ σc,eff min(δmax, δc,eff)`.
    exact: Real,
    /// Cohesive quadrature points.
    points: usize,
    /// Broken points (`δmax ≥ δc,eff`).
    broken: usize,
    /// Smallest and largest per-point `(total − reversible) / Gc`.
    ratio_range: (Real, Real),
}

/// Independent check of the cohesive law's dissipation (UD-803).
/// The linear law with `κ = 1` has the free energy
/// `ψ = ½ K(d) δ²`, `K = σc,eff (1 − d) / δmax`, so it dissipates exactly
/// `D = ½ σc,eff min(δmax, δc,eff) ≥ 0` at a point, whatever the path, and
/// a broken point (`δmax ≥ δc,eff`) dissipates `Gc = ½ σc,eff δc,eff`,
/// which is `½ σc δc` at every point (`δc,eff = σc / σc,eff · δc`). Per
/// point: the law's damage is `min(δmax / δc,eff, 1)` (bitwise),
/// `D ≥ 0`, and `|½ σc,eff δc,eff − ½ σc δc| ≤ 4u(1 + 2u)·½ σc δc` (two
/// roundings in the law, `δc,eff = fl(fl(σc/σc,eff)·δc)`, one in the check's own
/// product `½ σc,eff δc,eff`, one in the reference, `u = ε/2`). Total: with the
/// model's own integration, `∫ Gc·[broken] ≤ ∫ D ≤ ∫ Gc` **exactly** (the
/// integral is a fixed sequence of products by positive weights, sorts and
/// sums, each monotone in its inputs, and the integrands are ordered point
/// by point). And upstream's dissipated energy is `∫ (total − reversible)`
/// bitwise (measured). Returns both energies: they
/// differ when the time step does not resolve the softening.
fn assert_exact_dissipation(model: &mut SolidMechanicsModelCohesive) -> Dissipation {
    const TY: ElementType = ElementType::Cohesive3d6;
    const G: GhostType = GhostType::NotGhost;
    let upstream = model.energy("dissipated").unwrap();
    let law = model.cohesive_material("insertion").unwrap();
    let get = |name: &str| -> Vec<Real> {
        law.array(name, TY, G)
            .map(|a| a.as_slice().to_vec())
            .unwrap_or_default()
    };
    let (damage, delta_max, sigma_c, delta_c) = (
        get("damage"),
        get("delta max"),
        get("sigma_c_eff"),
        get("delta_c_eff"),
    );
    let (total, reversible) = (get("total_energy"), get("reversible_energy"));
    let filter: Vec<Idx> = law
        .element_filter()
        .get(TY, G)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default();
    let n = damage.len();
    for (name, a) in [
        ("delta max", &delta_max),
        ("sigma_c_eff", &sigma_c),
        ("delta_c_eff", &delta_c),
        ("total_energy", &total),
        ("reversible_energy", &reversible),
    ] {
        assert_eq!(a.len(), n, "{name}: one value per cohesive point");
    }
    let fe = model.smm.physics.base.fems.get(COHESIVE_FE_ENGINE).unwrap();
    let integrate = |f: &[Real]| -> Real {
        fe.integrate_scalar(ArrayView::new(f, 1).unwrap(), TY, G, Some(&filter))
            .unwrap()
    };
    let gc = 0.5 * SIGMA_C * DELTA_C;
    let u = Real::EPSILON / 2.0;
    let gc_bound = 4.0 * u * (1.0 + 2.0 * u) * gc;
    let (mut exact_q, mut lower_q, mut upper_q) = (vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    let (mut broken, mut lo, mut hi) = (0, Real::INFINITY, Real::NEG_INFINITY);
    for q in 0..n {
        let (sc, dc, dm) = (sigma_c[q], delta_c[q], delta_max[q]);
        assert!(
            sc > 0.0 && dc > 0.0 && dm >= 0.0,
            "point {q}: σc,eff {sc}, δc,eff {dc}, δmax {dm}"
        );
        assert_eq!(
            damage[q].to_bits(),
            (dm / dc).min(1.0).to_bits(),
            "point {q}: damage = min(δmax / δc,eff, 1)"
        );
        let gq = 0.5 * sc * dc;
        assert!(
            (gq - gc).abs() <= gc_bound,
            "point {q}: ½ σc,eff δc,eff = {gq} vs Gc = ½ σc δc = {gc}"
        );
        exact_q[q] = 0.5 * sc * dm.min(dc);
        assert!(exact_q[q] >= 0.0);
        upper_q[q] = gq;
        if dm >= dc {
            assert_eq!(damage[q], 1.0, "point {q}: broken");
            assert_eq!(exact_q[q], gq, "point {q}: a broken point dissipates Gc");
            lower_q[q] = gq;
            broken += 1;
        }
        let r = (total[q] - reversible[q]) / gc;
        lo = lo.min(r);
        hi = hi.max(r);
    }
    let (lower, exact, upper) = (
        integrate(&lower_q),
        integrate(&exact_q),
        integrate(&upper_q),
    );
    assert!(
        lower <= exact && exact <= upper,
        "∫ Gc·[broken] = {lower} ≤ ∫ D = {exact} ≤ ∫ Gc = {upper}"
    );
    let bookkeeping: Vec<Real> = total.iter().zip(&reversible).map(|(t, r)| t - r).collect();
    assert_eq!(
        upstream.to_bits(),
        integrate(&bookkeeping).to_bits(),
        "upstream's dissipated energy = ∫ (total − reversible)"
    );
    Dissipation {
        upstream,
        exact,
        points: n,
        broken,
        ratio_range: (lo, hi),
    }
}

/// Upstream's insertion criterion with an orthonormal facet frame (the
/// UD-217 correction), computed independently of the port's tangents: at
/// every quadrature point of the not-ghost `_triangle_3` facet `facet`,
/// `σ_eff / σc` for the facet stress averaged over both sides,
/// `t = σ̄ n`, `σn = t·n`, `τ² = |t|² − σn²`,
/// `σ_eff = √(max(σn, 0)² + τ²/β²)`, with the unit normal of the facet's
/// nodes. Needs `interpolate_stress` at the state to check.
fn corrected_ratios(model: &SolidMechanicsModelCohesive, facet: usize) -> Vec<Real> {
    let ty = ElementType::Triangle3;
    let g = GhostType::NotGhost;
    let mesh = model.smm.physics.base.mesh.read().unwrap();
    let fm = mesh.mesh_facets().unwrap();
    let nf = fm.nb_element(ty, g);
    let c = fm.connectivity(ty, g).unwrap().tuple(facet).to_vec();
    let nodes = mesh.nodes();
    let x = |k: usize| -> [Real; 3] {
        let i = usize::try_from(c[k]).unwrap();
        let p = nodes.tuple(i);
        [p[0], p[1], p[2]]
    };
    let (x0, x1, x2) = (x(0), x(1), x(2));
    let (a, b) = (
        [x1[0] - x0[0], x1[1] - x0[1], x1[2] - x0[2]],
        [x2[0] - x0[0], x2[1] - x0[1], x2[2] - x0[2]],
    );
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let norm = (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
    let nrm = cross.map(|v| v / norm);
    let fs = model.ext.facet_stress.get(ty, g).unwrap().as_slice();
    let nq = fs.len() / (nf * 18);
    assert_eq!(
        fs.len(),
        nf * nq * 18,
        "facet stress: 2 sides × 3 × 3 per point"
    );
    (0..nq)
        .map(|q| {
            let s = &fs[(facet * nq + q) * 18..(facet * nq + q + 1) * 18];
            let mut t = [0.0; 3];
            for (i, ti) in t.iter_mut().enumerate() {
                for k in 0..3 {
                    // column-major 3 × 3 of each side
                    *ti += (s[i + 3 * k] + s[9 + i + 3 * k]) / 2.0 * nrm[k];
                }
            }
            let sn = t[0] * nrm[0] + t[1] * nrm[1] + t[2] * nrm[2];
            let tau2 = (t[0] * t[0] + t[1] * t[1] + t[2] * t[2] - sn * sn).max(0.0);
            let sn = sn.max(0.0);
            (sn * sn + tau2 / (BETA * BETA)).sqrt() / SIGMA_C
        })
        .collect()
}

/// The three node coordinates of a facet, sorted (to find a facet by its
/// geometry in another model of the same mesh).
fn sorted_corners(nodes: &akantu::core::array::Array<Real>, conn: &[Idx]) -> Vec<[u64; 3]> {
    let mut v: Vec<[u64; 3]> = conn
        .iter()
        .map(|&n| {
            let p = nodes.tuple(usize::try_from(n).unwrap());
            [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()]
        })
        .collect();
    v.sort_unstable();
    v
}

/// The `user` sections of the two upstream input files.
#[test]
fn scalability_test_user_parameters() {
    let dir = scalability_test::example().source_dir;
    for (file, insertion) in [
        ("material-elastic.dat", false),
        ("material-cohesive.dat", true),
    ] {
        let p = akantu::parser::Parser::parse_file(dir.join(file)).unwrap();
        let u = scalability_test::user_parameters(&p).unwrap();
        assert_eq!(
            u,
            scalability_test::UserParameters {
                compression: -1e5,
                shear: -6e7,
                inc_shear: -1e6,
                output_energy: false,
                output_paraview: false,
                cohesive_insertion: insertion,
                max_steps: 5000,
                mesh: "cube.msh".into(),
            }
        );
    }
}

/// The cohesive law's parameters the independent checks use ([`SIGMA_C`],
/// [`DELTA_C`], [`BETA`]): those of upstream's `material-cohesive.dat` and
/// of the variant's input file.
#[test]
fn scalability_test_cohesive_law_parameters() {
    for input in [
        scalability_test::example()
            .source_dir
            .join("material-cohesive.dat"),
        probe_input("scalability_cohesive_full.dat"),
    ] {
        let text = std::fs::read_to_string(&input).unwrap();
        let law = text.split("material cohesive_linear [").nth(1).unwrap();
        let law = law.split(']').next().unwrap();
        let value = |key: &str| -> Real {
            law.lines()
                .find_map(|l| l.trim().strip_prefix(key)?.trim().strip_prefix('='))
                .unwrap_or_else(|| panic!("{}: {key}", input.display()))
                .split('#')
                .next()
                .unwrap()
                .trim()
                .parse()
                .unwrap()
        };
        assert_eq!(value("sigma_c"), SIGMA_C);
        assert_eq!(value("delta_c"), DELTA_C);
        assert_eq!(value("beta"), BETA);
    }
}

/// The oracle runs: three identical runs of each case (D-035).
#[test]
fn scalability_test_oracle_fixtures_are_consistent() {
    for case in [
        "scalability_test",
        "scalability_test_cohesive",
        "scalability_test_full",
        "scalability_test_cohesive_full",
    ] {
        assert_oracle_status(case);
    }
    // the upstream binary (MUMPS) and the Eigen variant print the same
    // 6-digit results (the variant's input file enables `output_energy`)
    let strip = |case: &str| -> Vec<String> {
        examples_cohesive_common::oracle_stdout(case)
            .into_iter()
            .filter(|l| !timing(l) && !l.starts_with(" - output_energy: "))
            .collect()
    };
    assert_eq!(strip("scalability_test"), strip("scalability_test_full"));
    assert_eq!(
        strip("scalability_test_cohesive"),
        strip("scalability_test_cohesive_full")
    );
}

/// Elastic configuration (the upstream ctest's input), 300 steps against
/// the Eigen-pinned oracle variant: the static solve, the time step and
/// the energies of every step bitwise (`Ew` accumulated, `Et`).
#[test]
fn scalability_test_elastic_matches_the_oracle_reduced() {
    let r = run(
        &probe_input("scalability_elastic_full.dat"),
        "scalability_elastic_reduced",
        Some(300),
    );
    assert_eq!(r.energies.len(), 301);
    assert_eq!(r.nb_cohesive_elements, 0);
    assert_plot_columns(&r.timings_csv, false);
    assert_energies("scalability_test_full", &r);
    let want = examples_cohesive_common::oracle_stdout("scalability_test_full");
    let dt = want.iter().find(|l| l.starts_with("Time step: ")).unwrap();
    assert!(r.stdout.contains(dt), "{dt}");
}

/// EX-049: the chronometer CSV (`printself_csv`) has the columns the
/// study's `plot.py` reads (`crates/akantu/examples/scalability_test/
/// plot.py`): `psize`, the time and repetitions of `solve_step` and
/// `static_solve`, and of `check_cohesive_stress` for a cohesive run; one
/// row with as many values as header fields, `psize` = 1.
fn assert_plot_columns(csv: &str, cohesive: bool) {
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 2, "{csv}");
    let header: Vec<&str> = lines[0].split(", ").map(|h| h.trim_matches('"')).collect();
    let values: Vec<&str> = lines[1].split(", ").collect();
    assert_eq!(header.len(), values.len(), "{csv}");
    assert_eq!((header[0], values[0]), ("psize", "1"));
    let mut need = vec![
        "solve_step",
        "solve_step nb_rep",
        "static_solve",
        "static_solve nb_rep",
    ];
    if cohesive {
        need.extend(["check_cohesive_stress", "check_cohesive_stress nb_rep"]);
    }
    for n in need {
        let k = header
            .iter()
            .position(|h| *h == n)
            .unwrap_or_else(|| panic!("{n} in {csv}"));
        assert!(values[k].parse::<f64>().is_ok(), "{n} = {}", values[k]);
    }
}

/// EX-049: dry run of the study's SLURM
/// scripts with stub `sbatch`, `srun` and `module` executables first on
/// `PATH` (each logs its arguments, tab-separated, one line per call):
/// `scalability.sh` submits upstream's 13 jobs (1 … 720 processes, 72 per
/// node: `--qos serial` on one node up to 72, `--qos parallel` with
/// `n / 72` full nodes beyond; job name `JOB_NAME`, default
/// `elastic_rust`; extra arguments before `--exclusive ./run.sh`), and
/// `run.sh` (whose `#SBATCH --time 1-0:0:0` precedes every command, so
/// SLURM reads it) restores the module set and runs the binary under
/// `srun` with `OMP_NUM_THREADS=1` and `--aka_input_file
/// ./material-elastic.dat` followed by its own arguments.
#[cfg(unix)]
#[test]
fn scalability_slurm_scripts_dry_run() {
    use std::os::unix::fs::PermissionsExt;
    let scripts =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/scalability_test");
    let out = scratch("scalability_slurm_dry_run");
    let bin = out.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for (stub, extra) in [
        ("sbatch", ""),
        ("srun", " OMP_NUM_THREADS=$OMP_NUM_THREADS"),
        ("module", ""),
    ] {
        let path = bin.join(stub);
        std::fs::write(
            &path,
            format!(
                "#!/bin/sh\n{{ printf '%s' \"{stub}{extra}\"; for a in \"$@\"; do printf '\\t%s' \"$a\"; done; \
                 printf '\\n'; }} >> \"$STUB_LOG\"\n"
            ),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let run = |script: &str, args: &[&str], env: &[(&str, &str)], log: &str| -> Vec<Vec<String>> {
        let log = out.join(log);
        let mut cmd = std::process::Command::new("bash");
        cmd.arg(scripts.join(script))
            .args(args)
            .current_dir(&out)
            .env("PATH", &path)
            .env("STUB_LOG", &log)
            .env_remove("JOB_NAME")
            .env_remove("AKANTU_RS_SCALABILITY_BIN");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let st = cmd.output().unwrap();
        assert!(
            st.status.success(),
            "{script}: {:?}\n{}",
            st.status,
            String::from_utf8_lossy(&st.stderr)
        );
        std::fs::read_to_string(&log)
            .unwrap_or_default()
            .lines()
            .map(|l| l.split('\t').map(str::to_owned).collect())
            .collect()
    };
    // scalability.sh: the 13 submissions
    let want = |job: &str, extra: &[&str]| -> Vec<Vec<String>> {
        [1, 2, 4, 9, 18, 36, 72, 144, 288, 432, 576, 648, 720]
            .iter()
            .map(|&n: &usize| {
                let (node, npn, qos) = if n <= 72 {
                    (1, n, "serial")
                } else {
                    (n / 72, 72, "parallel")
                };
                let mut v: Vec<String> = ["sbatch", "-J", job]
                    .iter()
                    .map(|s| (*s).to_owned())
                    .collect();
                v.extend([
                    "-e".to_owned(),
                    format!("{job}_%A.err"),
                    "-o".to_owned(),
                    format!("{job}_%A.out"),
                    "-N".to_owned(),
                    node.to_string(),
                    "--ntasks-per-node".to_owned(),
                    npn.to_string(),
                    "--qos".to_owned(),
                    qos.to_owned(),
                ]);
                v.extend(extra.iter().map(|s| (*s).to_owned()));
                v.extend(["--exclusive".to_owned(), "./run.sh".to_owned()]);
                v
            })
            .collect()
    };
    let calls = run("scalability.sh", &["--account", "lsms"], &[], "study.log");
    assert_eq!(calls, want("elastic_rust", &["--account", "lsms"]));
    let calls = run(
        "scalability.sh",
        &[],
        &[("JOB_NAME", "cohesive_rust")],
        "study2.log",
    );
    assert_eq!(calls, want("cohesive_rust", &[]));
    // run.sh: the #SBATCH directive comes before the first command
    let text = std::fs::read_to_string(scripts.join("run.sh")).unwrap();
    let first_command = text
        .lines()
        .position(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .unwrap();
    let directive = text
        .lines()
        .position(|l| l == "#SBATCH --time 1-0:0:0")
        .expect("#SBATCH --time");
    assert!(directive < first_command, "#SBATCH after a command");
    // run.sh with an explicit binary and extra arguments
    let calls = run(
        "run.sh",
        &["--aka_debug_level", "1"],
        &[
            (
                "AKANTU_RS_SCALABILITY_BIN",
                "/opt/akantu-rs/scalability_test",
            ),
            ("OMP_NUM_THREADS", "8"),
        ],
        "job.log",
    );
    let s = |v: &[&str]| -> Vec<String> { v.iter().map(|x| (*x).to_owned()).collect() };
    assert_eq!(
        calls,
        vec![
            s(&["module", "purge"]),
            s(&["module", "-q", "restore", "akantu-rs"]),
            s(&[
                "srun OMP_NUM_THREADS=1",
                "/opt/akantu-rs/scalability_test",
                "--aka_input_file",
                "./material-elastic.dat",
                "--aka_debug_level",
                "1"
            ]),
        ]
    );
    // the default binary: the cargo release build of the workspace
    let calls = run("run.sh", &[], &[], "job2.log");
    let srun = calls.last().unwrap();
    assert_eq!(srun[0], "srun OMP_NUM_THREADS=1");
    let suffix = "/../../../../target/release/examples/scalability_test";
    let prefix = srun[1]
        .strip_suffix(suffix)
        .unwrap_or_else(|| panic!("{}", srun[1]));
    assert_eq!(
        std::fs::canonicalize(prefix).unwrap(),
        std::fs::canonicalize(&scripts).unwrap()
    );
    assert_eq!(
        &srun[2..],
        &s(&["--aka_input_file", "./material-elastic.dat"])[..]
    );
}

/// Cohesive configuration up to step 1100 (the first cohesive element is
/// inserted at step 969): energies and cohesive-element counts of every
/// step bitwise against the Eigen-pinned oracle variant; the law's exact
/// dissipation (UD-803).
#[test]
fn scalability_test_cohesive_matches_the_oracle_up_to_the_first_insertions() {
    let mut r = run(
        &probe_input("scalability_cohesive_full.dat"),
        "scalability_cohesive_reduced",
        Some(1100),
    );
    assert_energies("scalability_test_cohesive_full", &r);
    assert_eq!(r.inserted.iter().position(|&n| n > 0), Some(968));
    assert!(r.nb_cohesive_elements > 0);
    assert_plot_columns(&r.timings_csv, true);
    let d = assert_exact_dissipation(&mut r.model);
    eprintln!("compat, 1100 steps: {d:?}");
    assert_eq!(d.points, 3 * r.nb_cohesive_elements);
    assert_eq!(d.upstream, r.energies[1100][0]);
    // already here, some points broke within one step from their insertion
    // traction: the trapezoid books a negative dissipation for one of them
    // (UD-803), while the law dissipates Gc at every broken point
    assert!(d.broken > 0 && d.exact > 0.0, "{d:?}");
    assert!(d.ratio_range.0 < 0.0, "{d:?}");
}

/// The port's default corrects UD-217 (orthonormal 3-D facet tangents):
/// the run is upstream's, bitwise, until upstream's first insertion (step
/// 969, an inclined facet whose upstream effective stress counts normal
/// traction as shear) and inserts later, first at step 1084
/// ([`scalability_test_default_rejects_upstreams_first_facet`] checks the
/// criterion at step 969); the law's exact dissipation (UD-803).
#[test]
fn scalability_test_default_corrects_ud217() {
    let mut r = run_with(
        &probe_input("scalability_cohesive_full.dat"),
        "scalability_cohesive_default",
        Some(1100),
        false,
    );
    let first = r.inserted.iter().position(|&n| n > 0);
    assert_eq!(first, Some(1083), "first insertion at step 1084");
    let d = assert_exact_dissipation(&mut r.model);
    eprintln!("default, 1100 steps: {d:?}");
    assert_eq!(d.points, 3 * r.nb_cohesive_elements);
    let o = oracle("scalability_test_cohesive_full");
    let lines = o.kept_lines("energies.csv");
    for (s, (e, l)) in r.energies.iter().zip(&lines[1..]).enumerate().take(969) {
        let c: Vec<&str> = l.split(", ").collect();
        assert_csv_values_bitwise(&format!("energies of step {s}"), e, &c[1..]);
    }
    let counts = o.kept_lines("cohesive_full.csv");
    assert_eq!(counts[969], "969, 1", "upstream inserts at step 969");
}

/// At step 969 (the state after step 968, bitwise upstream's), the
/// facet upstream inserts has a corrected effective stress below `σc` at
/// every quadrature point, and so has every facet the inserter checks:
/// with the UD-217 correction nothing is inserted at step 969 (independent
/// criterion: [`corrected_ratios`]).
#[test]
fn scalability_test_default_rejects_upstreams_first_facet() {
    let input = probe_input("scalability_cohesive_full.dat");
    // upstream's first cohesive element (compat run up to step 969)
    let up = run(&input, "scalability_first_facet_compat", Some(969));
    assert_eq!(up.inserted.iter().position(|&n| n > 0), Some(968));
    assert_eq!(up.nb_cohesive_elements, 1);
    let corners = {
        let m = up.mesh.read().unwrap();
        let c = m
            .connectivity(ElementType::Cohesive3d6, GhostType::NotGhost)
            .unwrap();
        sorted_corners(&m.nodes(), &c.tuple(0)[..3])
    };
    // the default model in the state checked at step 969
    let mut r = run_with(&input, "scalability_first_facet_default", Some(968), false);
    assert!(r.inserted.iter().all(|&n| n == 0));
    r.model.interpolate_stress().unwrap();
    let (ty, g) = (ElementType::Triangle3, GhostType::NotGhost);
    let (facet, checked) = {
        let m = r.mesh.read().unwrap();
        let fm = m.mesh_facets().unwrap();
        let conn = fm.connectivity(ty, g).unwrap();
        let found: Vec<usize> = (0..fm.nb_element(ty, g))
            .filter(|&f| sorted_corners(&m.nodes(), conn.tuple(f)) == corners)
            .collect();
        assert_eq!(
            found.len(),
            1,
            "upstream's first facet in the default model"
        );
        let checked: Vec<usize> = r
            .model
            .ext
            .inserter
            .check_facets()
            .get(ty, g)
            .unwrap()
            .as_slice()
            .iter()
            .enumerate()
            .filter_map(|(f, &c)| c.then_some(f))
            .collect();
        (found[0], checked)
    };
    assert!(checked.contains(&facet));
    let at_facet = corrected_ratios(&r.model, facet);
    eprintln!("upstream's first facet {facet}: corrected σ_eff/σc = {at_facet:?}");
    assert!(at_facet.iter().all(|&x| x < 1.0), "{at_facet:?}");
    let largest = checked
        .iter()
        .flat_map(|&f| corrected_ratios(&r.model, f))
        .fold(0.0, Real::max);
    eprintln!(
        "largest corrected σ_eff/σc at step 969 over {} checked facets: {largest}",
        checked.len()
    );
    assert!(largest < 1.0, "{largest}");
}

/// Medium tier: both whole runs (5000 steps) against the oracle variants
/// (every energy and count bitwise, standard output without the timings),
/// and the default runs (the port's default sparse solver) against the
/// upstream binary's standard output (6 digits; MUMPS upstream).
#[cfg(not(debug_assertions))]
#[test]
fn scalability_test_full_runs_match_the_oracle() {
    for (input, case) in [
        ("scalability_elastic_full.dat", "scalability_test_full"),
        (
            "scalability_cohesive_full.dat",
            "scalability_test_cohesive_full",
        ),
    ] {
        let mut r = run(&probe_input(input), case, None);
        assert_energies(case, &r);
        assert_eq!(r.energies.len(), 5001);
        examples_cohesive_common::assert_stdout_matches(case, &r.stdout, &timing);
        if r.nb_cohesive_elements > 0 {
            // UD-803: upstream's dissipated energy of this under-resolved
            // run is negative; the law dissipates a positive energy
            let d = assert_exact_dissipation(&mut r.model);
            eprintln!("{case}: {d:?}");
            assert!(d.upstream < 0.0 && d.exact > 0.0, "{d:?}");
            assert!(d.ratio_range.0 < 0.0, "{d:?}");
        }
    }
    // The corrected default (UD-217) over the whole run: first
    // insertion at step 1084, the law's exact dissipation (UD-803)
    let mut r = run_with(
        &probe_input("scalability_cohesive_full.dat"),
        "scalability_cohesive_default_full",
        None,
        false,
    );
    assert_eq!(r.inserted.iter().position(|&n| n > 0), Some(1083));
    let d = assert_exact_dissipation(&mut r.model);
    eprintln!("default (UD-217 corrected), 5000 steps: {d:?}");
    assert!(d.broken > 0 && d.exact > 0.0, "{d:?}");
    let dir = scalability_test::example().source_dir;
    for (input, case) in [
        ("material-elastic.dat", "scalability_test"),
        ("material-cohesive.dat", "scalability_test_cohesive"),
    ] {
        let r = run(&dir.join(input), &format!("{case}_default"), None);
        examples_cohesive_common::assert_stdout_matches(case, &r.stdout, &timing);
    }
}

/// `energies.csv` is written as upstream's
/// `fout << … << std::endl` (each line is in the file once written, so an
/// aborted run leaves the lines of the steps done) and holds the 6-digit
/// energies of step 0 and of every step.
#[test]
fn scalability_test_energies_csv_is_written_line_by_line() {
    let out = scratch("scalability_energies_csv");
    let path = out.join("lines.csv");
    let mut f = scalability_test::cohesive_common::LineFile::create(&path).unwrap();
    f.line("step, ed").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "step, ed\n");
    f.line("0, 0").unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "step, ed\n0, 0\n");
    drop(f);

    let mut ex = scalability_test::example().for_test(&out);
    ex.input_file = probe_input("scalability_elastic_full.dat");
    ex.steps = Some(3);
    ex.upstream_compat = true;
    let r = scalability_test::run(&ex).unwrap();
    let text = std::fs::read_to_string(out.join("energies.csv")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 1 + 4);
    assert_eq!(lines[0], "step, ed, ep, ek, ew, et");
    for (s, (l, e)) in lines[1..].iter().zip(&r.energies).enumerate() {
        let cols: Vec<String> = e
            .iter()
            .map(|&v| scalability_test::common::cout(v))
            .collect();
        assert_eq!(*l, format!("{s}, {}", cols.join(", ")));
    }
}

/// A standard-output line with its elapsed times
/// masked. With `output_paraview = true` upstream prints, every 100 steps,
/// a progress text ending in `\r` without a newline (`passing step s/n -
/// nb_cohesive_element: c - nb_dofs: d - <ms per step>\t\t \r`), so the
/// fragments of the whole loop make one line, and the first one carries
/// the chronometer's `Passing the first dumpers chrono! [<s>]`: each
/// `\r`-separated fragment keeps everything but its time.
fn mask_times(line: &str) -> String {
    // `str::lines` drops the `\r` that ends the oracle's progress line (the
    // `\r\n` of its last fragment and `std::endl`); the transcript keeps it
    let line = line.strip_suffix('\r').unwrap_or(line);
    line.split('\r')
        .map(|f| {
            if f.starts_with("passing step ") {
                match f.rfind(" - ") {
                    Some(i) => format!("{} - <ms>\t\t ", &f[..i]),
                    None => f.to_owned(),
                }
            } else if let Some(rest) = f.strip_prefix("Passing the first ") {
                match rest.find(" chrono! [") {
                    Some(i) => format!("Passing the first {} chrono! [<s>]", &rest[..i]),
                    None => f.to_owned(),
                }
            } else {
                f.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\r")
}

/// [`mask_times`] on the first progress line of a run: the time per step
/// and the chronometer value are masked, the rest is kept.
#[test]
fn scalability_test_progress_line_masking() {
    let line = "passing step 100/5000 - nb_cohesive_element: 0 - nb_dofs: 1236 - 0.0123\t\t \r\
                Passing the first dumpers chrono! [0.0004]";
    assert_eq!(
        mask_times(line),
        "passing step 100/5000 - nb_cohesive_element: 0 - nb_dofs: 1236 - <ms>\t\t \r\
         Passing the first dumpers chrono! [<s>]"
    );
}

/// The run of the ParaView variant `input` (`scalability_*_paraview.dat`,
/// both dumpers switched on) in its own output directory.
fn run_paraview(
    input: &str,
    name: &str,
    steps: Option<usize>,
    upstream_compat: bool,
) -> (scalability_test::Outcome, std::path::PathBuf) {
    let out = scratch(name);
    let mut ex = scalability_test::example().for_test(&out);
    ex.input_file = probe_input(input);
    ex.steps = steps;
    ex.upstream_compat = upstream_compat;
    let r = scalability_test::run(&ex).unwrap();
    (r, out)
}

/// Number of the dump written after explicit step `s` (dump 0 before the
/// static solve, dump 1 before the first explicit step, then every 100
/// steps).
fn dump_of_step(s: usize) -> usize {
    1 + s / 100
}

/// The ParaView files of a dump: both series (`extrinsic`, the solid
/// model's dumper; `solid_mechanics_model_cohesive`, the `cohesive
/// elements` dumper), `.pvtu` and `.vtu`.
fn dump_files(k: usize) -> Vec<String> {
    let mut v = Vec::new();
    for base in ["extrinsic", "solid_mechanics_model_cohesive"] {
        v.push(format!("paraview/{base}_{k:04}.pvtu"));
        v.push(format!("paraview/{base}-VTU/{base}_{k:04}.vtu"));
    }
    v
}

/// The oracle cases of the ParaView variants: three identical runs, 52
/// dumps of each dumper (`.pvtu`, `.vtu`) and the two `.pvd` files, and
/// the dumps change no value upstream (`energies.csv` and
/// `cohesive_full.csv` equal those of the variants without dumps).
#[test]
fn scalability_test_paraview_oracle_fixtures_are_consistent() {
    for (case, without) in [
        ("scalability_test_paraview", "scalability_test_full"),
        (
            "scalability_test_cohesive_paraview",
            "scalability_test_cohesive_full",
        ),
    ] {
        assert_oracle_status(case);
        let o = oracle(case);
        let w = oracle(without);
        for f in ["energies.csv", "cohesive_full.csv"] {
            assert_eq!(o.digests[f], w.digests[f], "{case}: {f}");
        }
        let dumps: Vec<String> = (0..52).flat_map(dump_files).collect();
        let mut want: Vec<&str> = dumps.iter().map(String::as_str).collect();
        want.extend([
            "paraview/extrinsic.pvd",
            "paraview/solid_mechanics_model_cohesive.pvd",
            "energies.csv",
            "cohesive_full.csv",
        ]);
        want.sort_unstable();
        let have: Vec<&str> = o.digests.keys().map(String::as_str).collect();
        assert_eq!(have, want, "{case}: output files");
    }
}

/// The `cohesive elements` piece `rel` of a dump without cohesive element,
/// written in the port's default layout, against the oracle's per-array
/// digests: every array equal, except that upstream's base64 stream of an
/// empty array is invalid (`undecodable` in the oracle's record, UD-114)
/// where the port writes a valid empty one.
fn assert_empty_cohesive_piece_ud114(
    o: &examples_common::oracle::OracleCase,
    out: &std::path::Path,
    rel: &str,
) {
    let mut empty = 0;
    for (name, ty, comps, nvalues, payload) in examples_common::oracle::vtu_payloads(&out.join(rel))
    {
        let want = &o.arrays[&(rel.to_owned(), name.clone())];
        assert_eq!((&ty, comps), (&want.ty, want.nb_component), "{rel}: {name}");
        if want.sha256 == "undecodable" {
            assert_eq!(nvalues, 0, "{rel}: {name} (UD-114: empty)");
            empty += 1;
        } else {
            assert_eq!(nvalues as i64, want.nb_values, "{rel}: {name}");
            assert_eq!(
                akantu_validation::checksum::sha256_bytes(&payload),
                want.sha256,
                "{rel}: {name}"
            );
        }
    }
    // connectivity, offsets, types and the cohesive tractions
    assert_eq!(empty, 4, "{rel}: empty arrays");
}

/// Item 1 of the ParaView check: with `output_paraview
/// = true` the example writes upstream's two series. Reduced runs (1100
/// steps: dumps 0…12; the first cohesive element is inserted at step 969):
/// with `--upstream-compat` every file written is byte-identical with the
/// oracle's (`.pvd` excepted: a reduced run's time series is a prefix). The
/// default (UD-217 corrected) gives upstream's `extrinsic` files up to dump
/// 10 (step 900) and upstream's `cohesive elements` pieces up to there
/// apart from the valid base64 of their empty arrays (UD-114); it departs
/// at dump 11 (step 1000), the first dump after upstream's first insertion
/// (step 969; the default inserts first at step 1084).
#[test]
fn scalability_test_paraview_dumps_match_the_oracle_reduced() {
    on_big_stack(paraview_dumps_match_the_oracle_reduced);
}

/// Runs `f` on a thread with a 16 MiB stack: in unoptimised builds with
/// `akantu-core/export-all` (gate tier `exportall`) the three runs of
/// [`scalability_test_paraview_dumps_match_the_oracle_reduced`] overflow the
/// test threads' 2 MiB of stack (measured: 3 MiB suffice; each run alone
/// fits, the example's own main thread has 8 MiB), as for
/// `examples_cohesive_contact_explicit_dynamic`. The need beyond a single run
/// comes from this test's own frame, which in an unoptimised build holds
/// several by-value `Outcome`s (about 64–69 KB each), not from a recursion of
/// the library.
fn on_big_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(16 << 20)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

fn paraview_dumps_match_the_oracle_reduced() {
    // `energies.csv`: 6 digits in the example, 17 in the oracle variant
    // (compared bitwise by the other tests); `cohesive_full.csv`: the
    // variant's record only
    let pvd = |f: &str| f.ends_with(".pvd") || f.ends_with(".csv");
    for (input, case) in [
        (
            "scalability_elastic_paraview.dat",
            "scalability_test_paraview",
        ),
        (
            "scalability_cohesive_paraview.dat",
            "scalability_test_cohesive_paraview",
        ),
    ] {
        let (_, out) = run_paraview(input, &format!("{case}_reduced"), Some(1100), true);
        let o = oracle(case);
        let n = o.assert_files_match(&out, false, &pvd);
        // the 13 dumps of both dumpers (.pvtu + .vtu)
        assert_eq!(n, 4 * (dump_of_step(1100) + 1), "{case}");
    }
    let (r, out) = run_paraview(
        "scalability_cohesive_paraview.dat",
        "scalability_test_cohesive_paraview_default",
        Some(1100),
        false,
    );
    assert_eq!(r.inserted.iter().position(|&n| n > 0), Some(1083));
    let o = oracle("scalability_test_cohesive_paraview");
    let before: Vec<String> = (0..=dump_of_step(900))
        .flat_map(dump_files)
        .filter(|f| !f.ends_with(".vtu") || f.contains("/extrinsic-VTU/"))
        .collect();
    let n = o.assert_files_match(&out, false, &|f| !before.iter().any(|b| b == f));
    assert_eq!(n, before.len());
    for k in 0..=dump_of_step(900) {
        let rel = format!(
            "paraview/solid_mechanics_model_cohesive-VTU/solid_mechanics_model_cohesive_{k:04}.vtu"
        );
        assert_empty_cohesive_piece_ud114(&o, &out, &rel);
    }
    let departed = dump_files(dump_of_step(1000))
        .into_iter()
        .filter(|f| f.ends_with(".vtu"))
        .filter(|f| examples_common::sha256(&out.join(f)) != o.digests[f])
        .count();
    assert_eq!(departed, 2, "UD-217: both pieces of dump 11 differ");
}

/// Medium tier (optimised builds only): the whole ParaView variants (5000
/// steps, 52 dumps of each dumper and the `.pvd` files) against the
/// oracle: with `--upstream-compat` (elastic and cohesive) every file
/// byte-identical (the CSV files are compared by the other tests); the
/// elastic run's default (no insertion, the cohesive switches are inert)
/// too, except the 52 `cohesive elements` pieces, which hold no cell and
/// differ only by the valid base64 of their empty arrays (UD-114); the
/// standard output line by line, elapsed times masked ([`mask_times`]).
#[cfg(not(debug_assertions))]
#[test]
fn scalability_test_paraview_full_runs_match_the_oracle() {
    for (input, case, compat) in [
        (
            "scalability_elastic_paraview.dat",
            "scalability_test_paraview",
            true,
        ),
        (
            "scalability_elastic_paraview.dat",
            "scalability_test_paraview",
            false,
        ),
        (
            "scalability_cohesive_paraview.dat",
            "scalability_test_cohesive_paraview",
            true,
        ),
    ] {
        let (r, out) = run_paraview(input, &format!("{case}_{compat}_full"), None, compat);
        let o = oracle(case);
        let coh = |f: &str| f.contains("/solid_mechanics_model_cohesive-VTU/");
        if compat {
            let csv = |f: &str| f.ends_with(".csv");
            assert_eq!(o.assert_files_match(&out, true, &csv), 4 * 52 + 2, "{case}");
        } else {
            let skip = |f: &str| f.ends_with(".csv") || coh(f);
            assert_eq!(
                o.assert_files_match(&out, true, &skip),
                3 * 52 + 2,
                "{case}"
            );
            for k in 0..52 {
                let rel = format!(
                    "paraview/solid_mechanics_model_cohesive-VTU/solid_mechanics_model_cohesive_{k:04}.vtu"
                );
                assert_empty_cohesive_piece_ud114(&o, &out, &rel);
            }
        }
        let want: Vec<String> = examples_cohesive_common::oracle_stdout(case)
            .into_iter()
            .filter(|l| !timing(l))
            .map(|l| mask_times(&l))
            .collect();
        let got: Vec<String> = r
            .stdout
            .iter()
            .filter(|l| !timing(l))
            .map(|l| mask_times(l))
            .collect();
        assert_eq!(got, want, "{case} (compat {compat}): standard output");
    }
}

/// UD-352 (D-064; UD-632 is a pointer to it), the static-then-explicit
/// pattern of this example: upstream's `initSolver` resets the current
/// position to `X` without invalidating it, so after `initNewSolver`
/// following a solve, `getCurrentPosition()` (and `getStableTimeStep()`)
/// use `X` instead of `X + u` whenever the position was refreshed at the
/// current displacement release. The port's default (`akantu-solid`)
/// copies `X` only into a newly allocated cache: the position,
/// the displacement and the stable time step are unchanged by the new
/// solver. (The example's oracle comparisons depend on the same rule: the
/// port refreshes the position during the static solve, upstream does not.)
#[test]
fn ud352_init_new_solver_keeps_the_current_position() {
    use akantu::core::enums::{AnalysisMethod, SpatialDirection};
    use akantu::model::boundary_condition::FixedValue;
    let mesh_file = examples_common::frozen_example_mesh("solid_mechanics_model/static/square.msh");
    let mut mesh = akantu::mesh::Mesh::new(2, "mesh").unwrap();
    akantu::io::read_mesh(&mut mesh, &mesh_file, akantu::core::enums::MeshIoType::Auto).unwrap();
    let x = mesh.nodes().as_slice().to_vec();
    let parser = akantu::parser::Parser::parse_file(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/static/material.dat"),
    )
    .unwrap();
    let mut model = akantu::solid::SolidMechanicsModel::with_parser(
        akantu::mesh::SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model
        .init_full(&akantu::model::ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let fixed = |value, axis| FixedValue { value, axis };
    model
        .apply_dirichlet_bc(&fixed(0.0, SpatialDirection::X), "Fixed_x")
        .unwrap();
    model
        .apply_dirichlet_bc(&fixed(0.0, SpatialDirection::Y), "Fixed_y")
        .unwrap();
    model
        .apply_dirichlet_bc(&fixed(1e-4, SpatialDirection::X), "Displacement")
        .unwrap();
    model.solve_step(None).unwrap();
    let u = model.displacement().unwrap().as_slice().to_vec();
    assert!(u.iter().any(|&v| v != 0.0));
    let deformed: Vec<f64> = x.iter().zip(&u).map(|(a, b)| a + b).collect();
    // refreshed at the current release
    assert_eq!(model.current_position().unwrap().as_slice(), &deformed[..]);
    let dt = model.stable_time_step().unwrap();
    model
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    assert_eq!(model.displacement().unwrap().as_slice(), &u[..]);
    assert_eq!(model.current_position().unwrap().as_slice(), &deformed[..]);
    assert_eq!(model.stable_time_step().unwrap(), dt);
}
