// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the `petsc/auto[petsc]` and
// `petsc/petsc_snes[petsc]` instances of
// test/test_model/patch_tests/patch_test_linear_gtest
// (patch_test_linear_solid_mechanics_fixture.hh `solver_options` under
// AKANTU_USE_PETSC; patch_test_linear_fixture.hh `SetUp` / `initModel`;
// patch_test_linear_elastic_{explicit,implicit}.cc,
// patch_test_linear_anisotropic_{explicit,implicit}.cc).
// (TEST-050…053).

//! The PETSc instances of upstream's patch tests (`TestPatchTestSMMLinear`,
//! typed list entries `{DOFManagerPETSc, TestSolverOptions<_auto, _petsc>}`
//! and `{DOFManagerPETSc, TestSolverOptions<_petsc_snes, _petsc>}`),
//! `harness = false`, feature `petsc`; run by the PETSc tier
//! (`scripts/petsc/run-petsc-tier.sh`) as a singleton, i.e. on one MPI
//! process like upstream's serial `patch_test_linear` (the mesh is serial,
//! so its PETSc DOF manager runs on the MPI world of one process, D-092).
//!
//! Every case of the Eigen replays of `model_smm_fixtures.rs` that is an
//! upstream instance runs twice on the PETSc DOF manager (upstream `SetUp`:
//! `model->initDOFManager("petsc")`; `smm_patch::PatchSolver::Petsc`):
//!
//! * `auto[petsc]`: the model's solver for the method (Newton–Raphson for
//!   the static and implicit cases, solving with a KSP with upstream's
//!   `pc_type cholesky` and `ksp_rtol 1e-30`; the lumped solver for the
//!   explicit cases) and upstream's Newton settings;
//! * `petsc_snes[petsc]`: PETSc SNES with upstream's `snes_max_it 10`,
//!   `snes_rtol 1e-10`, `snes_atol 2e-5`, `snes_stol 1e-10` (AnisotropicStatic:
//!   `snes_max_it 2`, `snes_rtol 1e-10`), for the static and implicit tests
//!   only: upstream skips `Explicit`, `ExplicitFiniteDeformation` and
//!   `AnisotropicExplicit` for `_petsc_snes` (`if (nls_type == _petsc_snes)
//!   GTEST_SKIP();`, `patch_test_linear_elastic_explicit.cc:26-27, 67-68`,
//!   `patch_test_linear_anisotropic_explicit.cc:32-33`), and so does this
//!   binary (`SKIP` lines). Runs: 128 (80
//!   `auto[petsc]` + 48 `petsc_snes[petsc]`), covering upstream's 208
//!   non-skipped instances; identical instances run once (UD-210: the
//!   finite-deformation tests read the small-strain files; anisotropic
//!   plane stress = plane strain).
//!
//! Criteria:
//!
//! * upstream's own: `checkAll` (DOFs 1e-10, gradient 1e-8, stresses 1e-9 /
//!   1e-11 anisotropic explicit, force balance 1e-8), `E_kin` (explicit
//!   1e-16, implicit 1e-13), and every solve converged (upstream throws
//!   otherwise: `NLSNotConvergedException`, `SNESNotConvergedException`);
//! * the DOF manager is the PETSc one and the solver the expected kind;
//! * bit for bit with the oracle's Eigen fixture: every value that no
//!   sparse solve produces (blocked DOFs, initial displacement, time steps,
//!   lumped mass), and every value of the explicit lumped-mass cases of
//!   `auto[petsc]` (no sparse solve); `K` and `M` are not compared (the
//!   PETSc backend has no COO access, as upstream's `SparseMatrixPETSc`);
//! * `auto[petsc]` static / implicit: the class of two backward-stable
//!   direct solves of the same matrix (a Cholesky factorisation as the KSP's
//!   preconditioner with `ksp_rtol 1e-30` against Eigen's SparseLU, as the
//!   MUMPS instances): equal Newton iteration counts and
//!   the final displacement within `1e-12` of its maximum;
//! * `petsc_snes[petsc]`: upstream's criteria only for the solved fields
//!   (SNES stops on its own tests, its inner KSP is PETSc's default
//!   iterative solver: no direct-solve class); the SNES iteration counts and
//!   the displacement's difference from the Eigen oracle are printed.
//!
//! Prints `failed=0` on success (the PETSc tier's pass criterion). On more
//! than one process every scenario reports "not applicable" (upstream runs
//! the patch tests on one process; a serial mesh's PETSc DOF manager is
//! refused there, D-092).

mod model_common;
mod smm_patch;

use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions};
use akantu_core::enums::NonLinearSolverType as Nls;
use akantu_solver::backend::BackendKind;
use smm_patch::{PatchSolver, direct_solver_class, replay_patch_with, smm_cases};

/// One upstream test of the typed suite and the fixture prefix of its cases.
struct Scenario {
    test: &'static str,
    prefix: &'static str,
    cases: usize,
    /// `true`: static or implicit dynamic (a Newton solve); `false`:
    /// explicit lumped mass.
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

fn instance_name(snes: bool) -> &'static str {
    if snes {
        "petsc/petsc_snes[petsc]"
    } else {
        "petsc/auto[petsc]"
    }
}

fn run_scenario(s: &Scenario, snes: bool) -> Result<(), String> {
    let cases = smm_cases(s.prefix);
    if cases.len() != s.cases {
        return Err(format!(
            "{} cases {}* (expected {})",
            cases.len(),
            s.prefix,
            s.cases
        ));
    }
    let solver = PatchSolver::Petsc { snes };
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
        if r.backend != Some(BackendKind::Petsc) {
            failures.push(format!("{c}: DOF manager backend {:?}", r.backend));
        }
        let want_nls = match (snes, s.solves) {
            (true, _) => Nls::PetscSnes,
            (false, true) => Nls::NewtonRaphson,
            (false, false) => Nls::Lumped,
        };
        if r.nls_kind != Some(want_nls) {
            failures.push(format!(
                "{c}: non-linear solver {:?}, expected {want_nls:?}",
                r.nls_kind
            ));
        }
        if !snes && let Some(m) = direct_solver_class(&r) {
            failures.push(format!("{c}: {m}"));
        }
        let its_sum: i64 = r.iterations.iter().sum();
        let ref_sum: i64 = r.reference_iterations.iter().sum();
        println!(
            "INFO {}[{}] {c}: nls {:?}, J solver {:?}, {} bitwise, iterations {its_sum} over {} \
             solves (Eigen oracle Newton {ref_sum}), u vs Eigen oracle: {} differing, max rel \
             {:.3e}, E_kin {}",
            s.test,
            instance_name(snes),
            r.nls_kind,
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
        std::thread::sleep(Duration::from_secs(2400));
        eprintln!("patch_petsc: watchdog expired");
        std::process::exit(70);
    });
    let world = Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(Duration::from_secs(600)),
        ..MpiOptions::default()
    })
    .expect("MPI world");
    let mut failed = 0;
    for snes in [false, true] {
        for s in &SCENARIOS {
            let t0 = Instant::now();
            if world.size() > 1 {
                if world.rank() == 0 {
                    println!(
                        "INFO not applicable at {} ranks: {}/{} (upstream runs the patch tests \
                         on one process)",
                        world.size(),
                        s.test,
                        instance_name(snes)
                    );
                }
                continue;
            }
            if snes && !s.solves {
                println!(
                    "SKIP TestPatchTestSMMLinear.{}/*/{} (upstream GTEST_SKIP for petsc_snes)",
                    s.test,
                    instance_name(snes)
                );
                continue;
            }
            match run_scenario(s, snes) {
                Ok(()) => println!(
                    "PASS TestPatchTestSMMLinear.{}/*/{} ({:.2} s)",
                    s.test,
                    instance_name(snes),
                    t0.elapsed().as_secs_f64()
                ),
                Err(e) => {
                    failed += 1;
                    println!(
                        "FAIL TestPatchTestSMMLinear.{}/*/{} ({:.2} s)\n{e}",
                        s.test,
                        instance_name(snes),
                        t0.elapsed().as_secs_f64()
                    );
                }
            }
        }
    }
    if world.rank() == 0 {
        println!("patch_petsc: failed={failed}");
    }
    drop(world);
    let _ = akantu_solver::model_solver::release_petsc_world();
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(i32::from(failed > 0));
}
