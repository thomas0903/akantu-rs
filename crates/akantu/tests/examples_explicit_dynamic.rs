// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/explicit_dynamic (port of
// examples/c++/solid_mechanics_model/explicit/explicit_dynamic.cc, upstream
// ctest `explicit_dynamic-test`).

mod examples_common;
#[path = "../examples/explicit_dynamic/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod explicit_dynamic;

use examples_common::{files_below, read_vtu, scratch};

/// Independent checks on a reduced run (20 steps): the initial pulse is
/// the upstream formula, the time step is `0.8 Δt_crit`, the free bar keeps
/// zero linear momentum (internal forces sum to zero), the total energy is
/// conserved to the accuracy of the central-difference scheme over 20
/// steps, `energy.csv` has one line per step in upstream's format and a
/// dump is written per step.
#[test]
fn explicit_dynamic_reduced_run() {
    let out = scratch("explicit_dynamic");
    let mut ex = explicit_dynamic::example().for_test(&out);
    ex.steps = Some(20);
    let r = explicit_dynamic::run(&ex).unwrap();
    assert_eq!(r.time_step, r.stable_time_step * 0.8);
    assert_eq!(r.energies.len(), 20);
    // upstream's first output line (the later ones name the step count)
    let o = examples_common::oracle::oracle("explicit_dynamic");
    examples_common::oracle::assert_stdout(
        "explicit_dynamic",
        &ex.transcript(),
        &o.stdout(),
        Some(1),
    );

    // momentum: Σ m v = 0 (free bar, zero initial velocity)
    let m_g = r.model.mass().unwrap();
    let m = m_g.as_slice();
    let v_g = r.model.velocity().unwrap();
    let v = v_g.as_slice();
    for d in 0..3 {
        let (mut p, mut scale) = (0.0, 0.0);
        for (mi, vi) in m.as_chunks::<3>().0.iter().zip(v.as_chunks::<3>().0.iter()) {
            p += mi[d] * vi[d];
            scale += (mi[d] * vi[d]).abs();
        }
        assert!(scale > 0.0 || d > 0, "the pulse moves the bar along x");
        assert!(
            p.abs() <= 1e-12 * scale.max(f64::MIN_POSITIVE),
            "momentum {d}: {p} vs {scale}"
        );
    }

    // energy: the pulse starts at rest, potential energy turns into kinetic
    // energy while the total stays put (O(Δt²) oscillation of the scheme)
    let tot: Vec<f64> = r.energies.iter().map(|e| e[0] + e[1]).collect();
    let (first, last) = (r.energies[0], r.energies[19]);
    assert!(
        last[1] > 10. * first[1],
        "kinetic energy grows: {first:?} → {last:?}"
    );
    for t in &tot {
        assert!(
            (t - tot[0]).abs() <= 1e-3 * tot[0],
            "total energy {t} vs {}",
            tot[0]
        );
    }

    // energy.csv: header + 20 lines `s,rtime,epot,ekin,tot,`
    let csv = std::fs::read_to_string(out.join("energy.csv")).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(lines.len(), 21);
    assert_eq!(lines[0], "id,rtime,epot,ekin,tot");
    assert!(
        lines[1].starts_with("1,") && lines[1].ends_with(','),
        "{}",
        lines[1]
    );

    // one dump per step plus the initial one (and the .pvd of the time step)
    let files = files_below(&out.join("paraview"));
    assert_eq!(files.len(), 2 * 21 + 1, "{files:?}");
    let vtu = read_vtu(&out.join("paraview/explicit_dynamic-VTU/explicit_dynamic_0020.vtu"));
    let du = vtu.array("displacement");
    assert_eq!(
        du.values.reals(),
        r.model.displacement().unwrap().as_slice()
    );
    assert_eq!(vtu.array("stress").nb_component, 9);
}

/// The initial displacement is the upstream sine × Gaussian pulse centred
/// at x = 2: zero at x = 2, odd around it, negligible at the far end of the
/// bar (x = −5: Gaussian factor e^(−(7k)²/4) ≈ 2e−5).
#[test]
fn explicit_dynamic_pulse_shape() {
    use explicit_dynamic::pulse;
    assert_eq!(pulse(2.0), 0.0);
    assert_eq!(pulse(2.5), -pulse(1.5));
    assert!(pulse(2.5) > 0.0);
    assert!(pulse(-5.0).abs() < 1e-3 * pulse(2.5));
}

/// Medium tier (optimised builds only): the full upstream run, 1000 steps
/// with a dump per step — total energy conserved over the run.
#[cfg(not(debug_assertions))]
#[test]
fn explicit_dynamic_full_run() {
    let out = scratch("explicit_dynamic_full");
    let ex = explicit_dynamic::example().for_test(&out);
    let r = explicit_dynamic::run(&ex).unwrap();
    assert_eq!(r.energies.len(), 1000);
    // the whole standard output equals the upstream binary's (time step,
    // `passing step` lines)
    let o = examples_common::oracle::oracle("explicit_dynamic");
    examples_common::oracle::assert_stdout("explicit_dynamic", &ex.transcript(), &o.stdout(), None);
    let tot: Vec<f64> = r.energies.iter().map(|e| e[0] + e[1]).collect();
    for t in &tot {
        assert!(
            (t - tot[0]).abs() <= 1e-3 * tot[0],
            "total energy {t} vs {}",
            tot[0]
        );
    }
    assert_eq!(files_below(&out.join("paraview")).len(), 2 * 1001 + 1);
}

/// The 17-digit `[epot, ekin]` of the oracle variant `explicit_dynamic_full`.
#[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
fn oracle_energies(n: usize) -> Vec<[String; 2]> {
    let o = examples_common::oracle::oracle("explicit_dynamic_full");
    o.kept_lines("energy_full.csv")[1..=n]
        .iter()
        .map(|l| {
            let c: Vec<&str> = l.split(',').collect();
            [c[2].to_owned(), c[3].to_owned()]
        })
        .collect()
}

/// Energies, `energy.csv` and dumps of `r` against the oracle runs.
#[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
fn compare_with_oracle(r: &explicit_dynamic::Outcome, out: &std::path::Path, complete: bool) {
    let n = r.energies.len();
    let want = oracle_energies(n);
    for (s, (e, w)) in r.energies.iter().zip(&want).enumerate() {
        examples_common::oracle::assert_csv_values_bitwise(
            &format!("energies after step {}", s + 1),
            e,
            &[w[0].as_str(), w[1].as_str()],
        );
    }
    let o = examples_common::oracle::oracle("explicit_dynamic");
    let csv = std::fs::read_to_string(out.join("energy.csv")).unwrap();
    let theirs = o.kept_lines("energy.csv");
    for (k, l) in csv.lines().enumerate() {
        assert_eq!(l, theirs[k], "energy.csv line {k}");
    }
    // dumps (the .pvd lists the dumps written so far: whole runs only)
    let skip = |f: &str| !complete && (f.ends_with(".pvd") || f == "energy.csv");
    let files = o.assert_files_match(out, complete, &skip);
    assert_eq!(files, if complete { 2 * 1001 + 2 } else { 2 * (n + 1) });
}

/// Oracle (x86_64 glibc: the initial pulse calls libm `sin`/`exp`), 20
/// steps: energies bitwise (17-digit variant), `energy.csv` lines and the
/// 21 dumps byte-identical to the upstream binary's.
#[cfg(all(target_os = "linux", target_env = "gnu", target_arch = "x86_64"))]
#[test]
fn explicit_dynamic_matches_the_oracle() {
    let out = scratch("explicit_dynamic_oracle");
    let mut ex = explicit_dynamic::example().for_test(&out);
    ex.steps = Some(20);
    let r = explicit_dynamic::run(&ex).unwrap();
    compare_with_oracle(&r, &out, false);
}

/// Medium tier, x86_64 glibc: the whole upstream run byte-identical (1001
/// dumps, `.pvd`, `energy.csv`), energies bitwise.
#[cfg(all(
    not(debug_assertions),
    target_os = "linux",
    target_env = "gnu",
    target_arch = "x86_64"
))]
#[test]
fn explicit_dynamic_full_run_matches_the_oracle() {
    let out = scratch("explicit_dynamic_full_oracle");
    let r = explicit_dynamic::run(&explicit_dynamic::example().for_test(&out)).unwrap();
    compare_with_oracle(&r, &out, true);
}
