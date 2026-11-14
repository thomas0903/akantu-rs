// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the `default/auto[mumps]` instances of
// test/test_model/patch_tests/patch_test_linear_gtest
// (patch_test_linear_solid_mechanics_fixture.hh `solver_options` under
// AKANTU_USE_MUMPS; patch_test_linear_elastic_{explicit,implicit}.cc,
// patch_test_linear_anisotropic_{explicit,implicit}.cc,
// patch_test_linear_fixture.hh).

//! The MUMPS instances of upstream's patch tests (`TestPatchTestSMMLinear`,
//! typed list entry `{DOFManagerDefault, TestSolverOptions<_auto,
//! _mumps>}`), `harness = false`, feature `mumps`; run by the mumps tier
//! (`scripts/mumps/run-mumps-tier.sh`) as a singleton, i.e. on one MPI
//! process like upstream's serial `patch_test_linear`.
//!
//! Every case of the Eigen replays of `model_smm_fixtures.rs` that is an
//! upstream instance runs again with `_sparse_solver_type = _mumps` on the
//! MPI world (`Explicit`, `ExplicitFiniteDeformation`, `Implicit`,
//! `ImplicitFiniteDeformation`, `Static`, `AnisotropicExplicit`,
//! `AnisotropicStatic`; `StaticFiniteDeformation` reads
//! `material_check_stress_plane_strain_finite_deformation.dat`, byte for
//! byte the plane-strain file of `Static` apart from
//! `finite_deformation = false` (UD-210), so the plane-strain `Static` cases
//! are its instances). Criteria (`smm_patch::PatchSolver::Mumps`):
//!
//! * upstream's own: `checkAll` (DOFs 1e-10, gradient 1e-8, stresses 1e-9 /
//!   1e-11 anisotropic explicit, force balance 1e-8) and `E_kin` (explicit
//!   1e-16, implicit 1e-13), Newton–Raphson converged at every solve (upstream
//!   throws otherwise);
//! * the installed sparse solver of `J` is MUMPS for the static and implicit
//!   cases (none for the explicit lumped-mass scheme);
//! * bit for bit with the oracle's Eigen fixture: every quantity that does
//!   not come from a sparse solve (blocked DOFs, initial displacement, time
//!   steps, `K`, `M`, lumped mass) and every value of the explicit cases;
//! * the class of two backward-stable direct solves of the same matrix
//!   (`K` is bitwise; NUMERICAL_VALIDATION, `mumps_mpi_solver`):
//!   the Newton iteration counts equal to the Eigen oracle
//!   run's and the final displacement within `1e-12` of its maximum
//!   (`smm_patch::direct_solver_class`; MUMPS and Eigen differ by rounding).
//!
//! Prints `failed=0` on success (the mumps tier's pass criterion). On more
//! than one process every scenario reports "not applicable" (upstream runs
//! the patch tests on one process).

mod model_common;
mod smm_patch;

use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions};
use akantu_core::enums::SparseSolverType;
use smm_patch::{PatchSolver, direct_solver_class, replay_patch_with, smm_cases};

/// One upstream test of the typed suite and the fixture prefix of its cases.
struct Scenario {
    test: &'static str,
    prefix: &'static str,
    cases: usize,
    /// `true`: static or implicit dynamic (Newton–Raphson with a sparse
    /// solve); `false`: explicit lumped mass.
    solves: bool,
}

const SCENARIOS: [Scenario; 7] = [
    Scenario {
        test: "Explicit",
        prefix: "patch_explicit_elastic_",
        cases: 16,
        solves: false,
    },
    Scenario {
        test: "ExplicitFiniteDeformation",
        prefix: "patch_explicit_fd_",
        cases: 4,
        solves: false,
    },
    Scenario {
        test: "Implicit",
        prefix: "patch_implicit_elastic_",
        cases: 16,
        solves: true,
    },
    Scenario {
        test: "ImplicitFiniteDeformation",
        prefix: "patch_implicit_fd_",
        cases: 4,
        solves: true,
    },
    Scenario {
        test: "Static",
        prefix: "patch_static_elastic_",
        cases: 16,
        solves: true,
    },
    Scenario {
        test: "AnisotropicExplicit",
        prefix: "patch_explicit_anisotropic_",
        cases: 12,
        solves: false,
    },
    Scenario {
        test: "AnisotropicStatic",
        prefix: "patch_static_anisotropic_",
        cases: 12,
        solves: true,
    },
];

fn run_scenario(s: &Scenario, world: &Communicator) -> Result<(), String> {
    let cases = smm_cases(s.prefix);
    if cases.len() != s.cases {
        return Err(format!(
            "{} cases {}* (expected {})",
            cases.len(),
            s.prefix,
            s.cases
        ));
    }
    let solver = PatchSolver::Mumps(world.clone());
    let mut failures = Vec::new();
    for c in &cases {
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            replay_patch_with(c, &solver)
        }));
        let r = match out {
            Ok(r) => r,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|m| (*m).to_owned()))
                    .unwrap_or_else(|| "panicked".into());
                failures.push(format!("{c}: {msg}"));
                continue;
            }
        };
        let want_kind = s.solves.then_some(SparseSolverType::Mumps);
        if r.sparse_kind != want_kind {
            failures.push(format!(
                "{c}: sparse solver of J {:?}, expected {want_kind:?}",
                r.sparse_kind
            ));
        }
        if let Some(m) = direct_solver_class(&r) {
            failures.push(format!("{c}: {m}"));
        }
        let its_sum: i64 = r.iterations.iter().sum();
        let ref_sum: i64 = r.reference_iterations.iter().sum();
        println!(
            "INFO {}[mumps] {c}: J solver {:?}, {} bitwise, Newton iterations {its_sum} \
             (Eigen oracle {ref_sum}, {} solves), u vs Eigen oracle: {} differing, max rel {:.3e}, \
             E_kin {}",
            s.test,
            r.sparse_kind,
            r.bitwise,
            r.iterations.len(),
            r.displacement_diff.0,
            r.displacement_diff.1,
            r.kinetic_energy
                .map_or_else(|| "-".to_owned(), |e| format!("{e:e}"))
        );
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(1800));
        eprintln!("patch_mumps: watchdog expired");
        std::process::exit(70);
    });
    let world = Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(Duration::from_secs(600)),
        ..MpiOptions::default()
    })
    .expect("MPI world");
    let mut failed = 0;
    for s in &SCENARIOS {
        let t0 = Instant::now();
        if world.size() > 1 {
            if world.rank() == 0 {
                println!(
                    "INFO not applicable at {} ranks: {} (upstream runs the patch tests on one \
                     process)",
                    world.size(),
                    s.test
                );
            }
            continue;
        }
        match run_scenario(s, &world) {
            Ok(()) => println!(
                "PASS TestPatchTestSMMLinear.{}/*/default/auto[mumps] ({:.2} s)",
                s.test,
                t0.elapsed().as_secs_f64()
            ),
            Err(e) => {
                failed += 1;
                println!(
                    "FAIL TestPatchTestSMMLinear.{}/*/default/auto[mumps] ({:.2} s)\n{e}",
                    s.test,
                    t0.elapsed().as_secs_f64()
                );
            }
        }
    }
    if world.rank() == 0 {
        println!("patch_mumps: failed={failed}");
    }
    drop(world);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(i32::from(failed > 0));
}
