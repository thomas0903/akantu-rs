// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/parsable_petsc.hh:55-67
// (updateInternalParameters: private database pushed around
// XSetFromOptions, then XSetFromOptions on the global database),
// test/test_model/test_phase_field_model/test_phase_field_tao.cc:51-52
// (PetscOptionsSetValue(NULL, ...)), src/solver/sparse_solver_petsc.cc:
// 72-78 (KSPSetUp failure); the partitioned MyModel toy of
// test/test_model/test_common/test_model_solver.

//! Isolated regression cases of the PETSc / TAO models (one `mpirun` each, run by
//! `scripts/petsc/run-petsc-regressions.sh`):
//!
//! * `gpe01_precedence` (INT-001): the global options
//!   database — command line ([`petsc::set_command_line`]),
//!   `PETSC_OPTIONS` (the runner exports `-tao_max_it 1` for this case) and
//!   [`petsc::set_global_option`] before / after the initialisation — is
//!   applied after the solver's private database and wins, for the KSP
//!   (only once a private option was set: UD-425), SNES and TAO; a
//!   rank-specific global option is refused on every rank.
//! * `gpe03_ksp_setup_failure` (SOLV-005): a `KSPSetUp` failure on
//!   every rank (`pc_type lu` with a factorisation package PETSc does not
//!   have) gives upstream's message, classified `Fatal { Transport }` (a
//!   PETSc error inside an agreed native collective; upstream throws an
//!   ordinary exception). A transport-class failure marks the process's
//!   communicators broken, so each rank prints `GPE03 CHECK PASS rank <r>`
//!   once its error is the expected one and then applies the runtime abort
//!   policy (`abort_on_transport_fatal`, `docs/ARCHITECTURE.md`); the runner
//!   passes the case on exit code 71 with rank 0's check line.
//! * `ape01_glue_panic`: a panic in the back-end's callback glue
//!   on one rank ends the job with `TRANSPORT_ABORT_CODE` (71); the runner
//!   passes it on that exit code and the glue panic's emergency note in the
//!   log within its time limit.
//! * `pt03_pending_global_option` (INT-001, PARSE-010,
//!   NLS-008): a global option set with [`petsc::set_global_option`] before
//!   the process's first PETSc use, with no command line and no
//!   `PETSC_OPTIONS` (the runner unsets it) to override it, is applied at
//!   PETSc's initialisation: `tao_max_it 1` makes the bounded TAO step (4
//!   GPCG iterations without the limit) stop at 1 iteration. Upstream's
//!   order (TEST-100: `PetscOptionsSetValue(NULL, …)` before
//!   `initDOFManager("petsc")`); a mutant that never applies the pending
//!   options fails it.

use akantu_comm::Communicator;
use akantu_core::{Error, FatalKind};
use akantu_linalg::petsc;
use akantu_parser::param::ParamValue;
use akantu_solver::petsc_backend::inject_glue_panic;

use crate::petsc_pftao::tao_bounds::{bounded_bar, bounded_step};
use crate::{KspConf, Outcome, Setup, build, e2s, petsc_backend};

/// Iterations of a solve that must stop at `max_it` (`Error::NotConverged`).
fn stopped_at(r: &akantu_core::Result<()>) -> Option<usize> {
    match r {
        Err(Error::NotConverged { niter, .. }) => Some(*niter),
        _ => None,
    }
}

/// The SNES cubic bar (private `snes_max_it 50`): iterations at which it
/// stopped (`None`: converged or another error).
fn snes_stop(c: &Communicator) -> Result<Option<usize>, String> {
    let (mut model, mut ms) = build(c, Setup::SnesCubic, true, KspConf::Direct)?;
    let r = ms.solve_step(&mut model, None);
    if c.rank() == 0 {
        println!("INFO gpe01: SNES cubic bar: {r:?}");
    }
    Ok(stopped_at(&r))
}

/// `gpe01_*` (see the module documentation).
pub fn gpe01_precedence(c: &Communicator) -> Outcome {
    if std::env::var("PETSC_OPTIONS").as_deref() != Ok("-tao_max_it 1") {
        return Err(
            "PETSC_OPTIONS must be '-tao_max_it 1' (run through run-petsc-regressions.sh)".into(),
        );
    }
    let mut fails = Vec::new();
    // Before the process's first PETSc use: an identical command line, and a
    // global option set before the initialisation (applied before
    // PetscInitialize, so the command line wins over it, as upstream).
    petsc::set_command_line(
        ["petsc_mpi", "-snes_max_it", "1", "-ksp_type", "cg"]
            .into_iter()
            .map(String::from),
    );
    petsc::set_global_option(c, "snes_max_it", "3").map_err(e2s)?;
    // SNES: private snes_max_it 50, pending global 3, command line 1.
    let s1 = snes_stop(c)?;
    if s1 != Some(1) {
        fails.push(format!(
            "SNES with -snes_max_it 1 on the command line: stopped at {s1:?}"
        ));
    }
    // TAO: private tao_max_it 50, PETSC_OPTIONS -tao_max_it 1: the
    // unbounded step converges in one iteration, the bounded one (4
    // iterations without the limit: petsc_tao_bounds) stops at 1.
    {
        let (mut model, mut ms) = bounded_bar(c)?;
        ms.non_linear_solver_mut(None)
            .map_err(e2s)?
            .set_param("tao_max_it", ParamValue::Int(50))
            .map_err(e2s)?;
        ms.solve_step(&mut model, None).map_err(e2s)?;
        bounded_step(&mut model, &mut ms)?;
        let r = ms.solve_step(&mut model, None);
        if c.rank() == 0 {
            println!("INFO gpe01: bounded TAO step: {r:?}");
        }
        if stopped_at(&r) != Some(1) {
            fails.push(format!("TAO with PETSC_OPTIONS -tao_max_it 1: {r:?}"));
        }
    }
    // KSP without any private option: KSPSetFromOptions is never called
    // (UD-425), the command line's -ksp_type cg has no effect.
    {
        let (mut model, mut ms) = build(c, Setup::StaticNewton, true, KspConf::Default)?;
        ms.solve_step(&mut model, None).map_err(e2s)?;
        let (ksp, pc) = petsc_backend(&mut ms.dof_manager)?
            .ksp_types("J")
            .map_err(e2s)?;
        if c.rank() == 0 {
            println!("INFO gpe01: KSP without a private option: {ksp} / {pc}");
        }
        if ksp != "gmres" {
            fails.push(format!(
                "KSP without a private option: type {ksp} (want gmres)"
            ));
        }
    }
    // KSP with private options (preonly + LU): the global cg wins.
    {
        let (mut model, mut ms) = build(c, Setup::StaticNewton, true, KspConf::Direct)?;
        ms.solve_step(&mut model, None).map_err(e2s)?;
        let (ksp, pc) = petsc_backend(&mut ms.dof_manager)?
            .ksp_types("J")
            .map_err(e2s)?;
        if c.rank() == 0 {
            println!("INFO gpe01: KSP with private preonly + lu: {ksp} / {pc}");
        }
        if ksp != "cg" || pc != "lu" {
            fails.push(format!(
                "KSP with private options: {ksp} / {pc} (want cg / lu)"
            ));
        }
    }
    // After the initialisation: a global option replaces the command line's.
    petsc::set_global_option(c, "snes_max_it", "2").map_err(e2s)?;
    let s2 = snes_stop(c)?;
    if s2 != Some(2) {
        fails.push(format!(
            "SNES after set_global_option(snes_max_it, 2): stopped at {s2:?}"
        ));
    }
    // A rank-specific global option is refused on every rank (and changes
    // nothing).
    if c.size() > 1 {
        let v = if c.rank() == 0 { "5" } else { "6" };
        match petsc::set_global_option(c, "snes_max_it", v) {
            Err(Error::InvalidArgument(_)) => {}
            other => fails.push(format!("rank-specific global option: {other:?}")),
        }
        let s3 = snes_stop(c)?;
        if s3 != Some(2) {
            fails.push(format!("SNES after a refused option: stopped at {s3:?}"));
        }
    }
    agree(c, fails)
}

/// `pt03_*` (see the module documentation).
pub fn pt03_pending_global_option(c: &Communicator) -> Outcome {
    if std::env::var_os("PETSC_OPTIONS").is_some() {
        return Err("PETSC_OPTIONS must be unset (run through run-petsc-regressions.sh)".into());
    }
    let mut fails = Vec::new();
    // Before the process's first PETSc use (no command line set): queued,
    // applied right before PetscInitialize.
    petsc::set_global_option(c, "tao_max_it", "1").map_err(e2s)?;
    let (mut model, mut ms) = bounded_bar(c)?;
    // The unbounded step converges in one GPCG iteration.
    let r1 = ms.solve_step(&mut model, None);
    if let Err(e) = &r1 {
        fails.push(format!("unbounded step: {e}"));
    }
    // The bounded step needs 4 without the limit: stops at 1.
    bounded_step(&mut model, &mut ms)?;
    let r2 = ms.solve_step(&mut model, None);
    if c.rank() == 0 {
        println!(
            "INFO pt03: tao_max_it 1 set before PETSc's initialisation: unbounded step {r1:?}, \
             bounded step {r2:?}"
        );
    }
    if stopped_at(&r2) != Some(1) {
        fails.push(format!(
            "bounded TAO step with the pending global tao_max_it 1: {r2:?} (want NotConverged \
             after 1 iteration)"
        ));
    }
    agree(c, fails)
}

/// `gpe03_*` (see the module documentation).
pub fn gpe03_ksp_setup_failure(c: &Communicator) -> Outcome {
    // A duplicate for the failing solve (a transport-class failure marks
    // the process's communicators broken: the case aborts, see above).
    let w = c.duplicate().map_err(e2s)?;
    let (mut model, mut ms) = build(&w, Setup::StaticLinear, true, KspConf::Default)?;
    for (k, v) in [
        ("ksp_type", "preonly"),
        ("pc_type", "lu"),
        ("pc_factor_mat_solver_type", "akantu_no_such_package"),
    ] {
        ms.dof_manager
            .backend_mut()
            .map_err(e2s)?
            .set_solver_option("J", k, v)
            .map_err(e2s)?;
    }
    let r = ms.solve_step(&mut model, None);
    match &r {
        Err(e) => {
            let m = e.to_string();
            if c.rank() == 0 {
                println!("INFO gpe03: {m}");
            }
            if e.fatal_kind() == Some(FatalKind::Transport)
                && m.contains(
                    "PETSc KSP sparse solver initialization failed: is your matrix singular ?\n",
                )
            {
                println!("GPE03 CHECK PASS rank {}", c.rank());
                // The runtime abort policy of a transport-class failure
                // (never returns).
                w.abort_on_transport_fatal(e);
            }
            Err(format!("unexpected error {e:?}"))
        }
        Ok(()) => Err("the solve succeeded".into()),
    }
}

/// `ape01_*` (see the module documentation): never returns on success.
pub fn ape01_glue_panic(c: &Communicator) -> Outcome {
    let (mut model, mut ms) = build(c, Setup::SnesLinear, true, KspConf::Direct)?;
    let rank = i32::try_from(c.size() - 1).map_err(|e| e.to_string())?;
    inject_glue_panic(rank);
    let r = ms.solve_step(&mut model, None);
    Err(format!(
        "the job was not aborted after the injected glue panic on rank {rank}: {r:?}"
    ))
}

fn agree(c: &Communicator, fails: Vec<String>) -> Outcome {
    let mut ok = [u8::from(fails.is_empty())];
    c.all_reduce(&mut ok, akantu_comm::ReduceOp::Min)
        .map_err(e2s)?;
    if !fails.is_empty() {
        return Err(fails.join("; "));
    }
    if ok[0] != 1 {
        return Err("failed on another rank".into());
    }
    Ok(())
}
