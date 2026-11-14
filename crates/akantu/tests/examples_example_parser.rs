// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Integration test of
// the example crates/akantu/examples/example_parser (port of
// examples/c++/solid_mechanics_model/io/parser/example_parser.cc, upstream
// ctest `example_parser-test`).

#[path = "../examples/example_parser/main.rs"]
#[allow(dead_code)] // the example's `main` is not called by the test
mod example_parser;
mod examples_common;

use examples_common::{files_below, read_vtu, rel_diff, scratch};

/// The user parameters are read from the `user parameters` section (the
/// stress matrix is an expression of another user parameter), the plate is
/// clamped on the crust and loaded by the pressure on the holes; checks:
/// the pressure on the closed holes has no resultant, the solved state is
/// in equilibrium (`f_ext + f_int = 0` on the free DOFs) and the crust
/// reactions balance the hole loads; the displacement is dumped once.
#[test]
fn example_parser_reads_the_user_section_and_solves() {
    let out = scratch("example_parser");
    let r = example_parser::run(&example_parser::example().for_test(&out)).unwrap();
    let u = &r.user;
    assert_eq!(u.spatial_dimension, 2);
    assert_eq!(u.mesh_file, "swiss_cheese.msh");
    assert_eq!(
        (u.outer_crust.as_str(), u.inner_holes.as_str()),
        ("crust", "holes")
    );
    assert_eq!(u.max_iter, 100);
    assert_eq!(u.precision, 1e-9);
    assert_eq!((u.eigen_stress.rows(), u.eigen_stress.cols()), (2, 2));
    assert_eq!(u.eigen_stress.as_slice(), &[30e3, 0.0, 0.0, 30e3]);

    let model = &r.model;
    let f_ext_g = model.external_force().unwrap();
    let f_ext = f_ext_g.as_slice();
    let f_int_g = model.internal_force().unwrap();
    let f_int = f_int_g.as_slice();
    let blocked_g = model.blocked_dofs().unwrap();
    let blocked = blocked_g.as_slice();
    let f_scale = f_ext.iter().fold(0.0_f64, |m, f| m.max(f.abs()));
    assert!(f_scale > 0.0);
    for d in 0..2 {
        let resultant: f64 = f_ext.iter().skip(d).step_by(2).sum();
        // Σ over closed polygons of σ·n·l = 0, up to rounding
        assert!(
            resultant.abs() <= 1e-12 * f_scale,
            "load resultant {d}: {resultant}"
        );
    }
    let mut reaction = [0.0, 0.0];
    for (k, ((fe, fi), b)) in f_ext.iter().zip(f_int).zip(blocked).enumerate() {
        if *b {
            reaction[k % 2] += fi;
        } else {
            // linear static solve with threshold 1e-9: residual at rounding
            assert!(
                (fe + fi).abs() <= 1e-9 * f_scale,
                "dof {k}: residual {}",
                fe + fi
            );
        }
    }
    for (d, r) in reaction.iter().enumerate() {
        assert!(r.abs() <= 1e-9 * f_scale, "crust reaction {d}: {r}");
    }

    assert_eq!(
        files_below(&out.join("paraview")),
        [
            "swiss_cheese-VTU/swiss_cheese_0000.vtu",
            "swiss_cheese_0000.pvtu"
        ]
    );
    let vtu = read_vtu(&out.join("paraview/swiss_cheese-VTU/swiss_cheese_0000.vtu"));
    let du = vtu.array("displacement").values.reals();
    let u_g = model.displacement().unwrap();
    let u = u_g.as_slice();
    for (n, v) in du.as_chunks::<3>().0.iter().enumerate() {
        assert_eq!([v[0], v[1], v[2]], [u[2 * n], u[2 * n + 1], 0.0]);
    }
}

/// The example with the Eigen solver pinned, outputs below `out`.
fn run_eigen(out: &std::path::Path) -> example_parser::Outcome {
    let mut ex = example_parser::example().for_test(out);
    ex.sparse_solver = Some(akantu::core::enums::SparseSolverType::Eigen);
    example_parser::run(&ex).unwrap()
}

/// Oracle variant `example_parser_eigen` (Eigen solver pinned, oracle with
/// Eigen's cache sizes pinned to 16384/524288/524288, D-050): the assembled
/// load (`external_force.csv`, 17 digits) bitwise, the same number of
/// Newton iterations, and every output file (the dump, displacement
/// included) **byte-identical** (the former 1e-12 rule
/// covered the oracle's run-time cache query).
#[test]
fn example_parser_eigen_matches_the_oracle() {
    use examples_common::oracle::{assert_csv_values_bitwise, oracle};
    let out = scratch("example_parser_eigen_oracle");
    let r = run_eigen(&out);
    let o = oracle("example_parser_eigen");
    let f_g = r.model.external_force().unwrap();
    let f = f_g.as_slice();
    let lines = o.kept_lines("external_force.csv");
    assert_eq!(lines.len() * 2, f.len());
    for (n, l) in lines.iter().enumerate() {
        let c: Vec<&str> = l.split(',').collect();
        assert_csv_values_bitwise(
            &format!("external_force of node {n}"),
            &f[2 * n..2 * n + 2],
            &c,
        );
    }
    let newton = o.kept_lines("newton.txt");
    let nls = r.model.solver.non_linear_solver(None).unwrap();
    assert_eq!(newton[0], format!("nb_iterations {}", nls.nb_iterations()));
    assert_eq!(o.assert_files_match(&out, false, &|_| false), 2);
    let rel = "paraview/swiss_cheese-VTU/swiss_cheese_0000.vtu";
    assert_eq!(o.assert_vtu_arrays_match(&out, rel, &|_| false), 5);
}

/// Diagnostic (D-050 (3)): the recorded variant
/// `example_parser_eigen_unpinned` — the same oracle variant with Eigen's
/// cache sizes queried from the x86_64 host (L1 ≥ 32 KiB: no depth blocking
/// in the supernodal panel updates). The arrays that do not depend on the
/// solve are bitwise; the displacement difference is printed (3,840 of
/// 4,512 values, 1.75e-15).
#[test]
fn example_parser_eigen_unpinned_oracle_diagnostic() {
    let out = scratch("example_parser_eigen_unpinned");
    run_eigen(&out);
    let o = examples_common::oracle::oracle("example_parser_eigen_unpinned");
    let rel = "paraview/swiss_cheese-VTU/swiss_cheese_0000.vtu";
    assert_eq!(
        o.assert_vtu_arrays_match(&out, rel, &|n| n == "displacement"),
        4
    );
    let mine = read_vtu(&out.join(rel));
    let theirs = read_vtu(&o.kept(rel));
    let (a, b) = (
        mine.array("displacement").values.reals(),
        theirs.array("displacement").values.reals(),
    );
    let differ = a
        .iter()
        .zip(b)
        .filter(|(x, y)| x.to_bits() != y.to_bits())
        .count();
    println!(
        "example_parser vs the unpinned Eigen oracle: displacement {differ}/{} values differ, \
         max relative difference {:e}",
        a.len(),
        rel_diff(a, b)
    );
}

/// Oracle, upstream binary (MUMPS): mesh arrays bitwise, the solved
/// displacement diagnostic (printed).
#[test]
fn example_parser_mumps_oracle_diagnostic() {
    let out = scratch("example_parser_mumps_oracle");
    example_parser::run(&example_parser::example().for_test(&out)).unwrap();
    let o = examples_common::oracle::oracle("example_parser");
    let rel = "paraview/swiss_cheese-VTU/swiss_cheese_0000.vtu";
    assert_eq!(
        o.assert_vtu_arrays_match(&out, rel, &|n| n == "displacement"),
        4
    );
    let d = rel_diff(
        read_vtu(&out.join(rel))
            .array("displacement")
            .values
            .reals(),
        read_vtu(&o.kept(rel)).array("displacement").values.reals(),
    );
    println!("example_parser (MUMPS oracle) displacement: max relative difference {d:e}");
}
