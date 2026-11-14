// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs (ported intent: the PARALLEL registration of
// test_solid_mechanics_model and test_dof_mesh_distribute.cc): solid
// mechanics on distributed meshes on real MPI; also the PARALLEL upstream
// tests test/test_synchronizer/test_dof_synchronizer.cc on bar.msh and
// test/test_model/test_solid_mechanics_model/
// test_solid_mechanics_model_reassign_material.cc; node
// removal on a rank boundary (SMM-015, no upstream test).

//! Real-MPI SMM scenarios (harness = false, `mpirun -np N mpi_smm`;
//! scripts/mpi/run-par-tests.sh). `PASS`/`FAIL` lines for the scenarios
//! that ran, `NOT RUN <name>: <why>` for those that do not run at this
//! rank count and a `SUMMARY … not_run=K` on rank 0
//! (`scenarios=` counts the scenarios that ran); exit 1 on failure.

#[path = "../../akantu-solver/tests/dof_sync_scenario/mod.rs"]
mod dof_sync_scenario;
mod node_removal_common;
mod non_local_common;
#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_laws;
mod par_nl;
mod par_oracle;
mod par_smm;
mod reassign_scenario;
mod smm_bar_dynamics;

use std::time::Duration;

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp, ReductionPolicy, SendMode};
use par_smm::{Outcome, Part, Run};

fn world_with(reduction: ReductionPolicy) -> Communicator {
    let s = std::env::var("AKANTU_MPI_TEST_TIMEOUT_S")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(60);
    Communicator::mpi_world(MpiOptions {
        thread_level: ThreadLevel::Funneled,
        timeout: Some(Duration::from_secs(s)),
        send_mode: SendMode::Eager,
        reduction,
    })
    .expect("MPI world communicator")
}

fn world() -> Communicator {
    world_with(ReductionPolicy::RankOrdered)
}

/// `TestSMMFixtureBarImplicit.Dynamics` under upstream's `PARALLEL`
/// registration (`test_solid_mechanics_model/CMakeLists.txt:72-80`; TEST-109):
/// the 1-D and 2-D bars (the 3-D ones need optimised builds, see
/// `upstream_solid_mechanics_model_dynamics.rs`) on this world, RCB
/// partition, upstream's skip of `_segment_2` above 2 ranks; upstream's
/// `EXPECT_NEAR(max_error, 0, 2e-3)` on every rank, and on rank 0 the
/// gathered final `u`, `v` against the serial run within `1e-12` of their
/// maximum (the explicit vs-serial value, a regression
/// guard; one rank: bitwise).
fn smm_bar_dynamics_implicit_vs_serial(c: Communicator) -> Outcome {
    use akantu_core::ElementType as T;
    use akantu_core::enums::AnalysisMethod::ImplicitDynamic as Im;
    for ty in [
        T::Segment2,
        T::Segment3,
        T::Triangle3,
        T::Triangle6,
        T::Quadrangle4,
        T::Quadrangle8,
    ] {
        if ty == T::Segment2 && c.size() > 2 {
            if c.rank() == 0 {
                println!(
                    "INFO {ty} implicit at {} ranks: skipped (upstream)",
                    c.size()
                );
            }
            continue;
        }
        let run = smm_bar_dynamics::run_bar(c.clone(), ty, Im)?;
        if run.max_error.abs() > 2e-3 {
            return Err(format!("{ty}: max error {}", run.max_error));
        }
        if c.rank() == 0 {
            let ser = smm_bar_dynamics::run_bar(Communicator::serial(), ty, Im)?;
            let line = smm_bar_dynamics::compare_with_serial(&run, &ser, 1e-12)
                .map_err(|m| format!("{ty}: {m}"))?;
            if c.size() == 1 && (run.u != ser.u || run.v != ser.v) {
                return Err(format!("{ty}: one rank must equal the serial run bitwise"));
            }
            println!(
                "INFO {ty} implicit at {} ranks: max error {:e}; vs serial: {line}",
                c.size(),
                run.max_error
            );
        }
    }
    Ok(())
}

/// A scenario that runs only at some rank counts: its name and function.
type Named = (&'static str, fn(Communicator) -> Outcome);

fn main() {
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(900));
        eprintln!("WATCHDOG: no completion after 900 s, aborting");
        std::process::abort();
    });
    let control = world();
    type S = Box<dyn Fn(Communicator) -> Outcome>;
    let scenarios: Vec<(&str, S)> = vec![
        (
            "explicit_distributed_vs_serial",
            Box::new(|c| par_smm::distributed_vs_serial(c, Run::Explicit, 4, 40, Part::Rcb, 1e-12)),
        ),
        (
            "static_distributed_vs_serial",
            Box::new(|c| par_smm::distributed_vs_serial(c, Run::Static, 3, 1, Part::Rcb, 1e-10)),
        ),
        (
            "implicit_dynamic_distributed_vs_serial",
            Box::new(|c| {
                par_smm::distributed_vs_serial(c, Run::ImplicitDynamic, 3, 5, Part::Rcb, 1e-10)
            }),
        ),
        (
            "test_dof_mesh_distribute",
            Box::new(par_smm::dof_mesh_distribute),
        ),
        (
            "model_synchronize_tags",
            Box::new(par_smm::model_synchronize_tags),
        ),
        // SMM-015: a removal on a rank boundary equals a
        // fresh model (bitwise), and its rollback when one rank refuses
        (
            "node_removal_explicit_rank_boundary",
            Box::new(|c| {
                node_removal_common::rank_boundary_equivalence(
                    c,
                    node_removal_common::Method::Explicit,
                    4,
                )
            }),
        ),
        (
            "node_removal_static_rank_boundary",
            Box::new(|c| {
                node_removal_common::rank_boundary_equivalence(
                    c,
                    node_removal_common::Method::Static,
                    1,
                )
            }),
        ),
        (
            "node_removal_implicit_dynamic_rank_boundary",
            Box::new(|c| {
                node_removal_common::rank_boundary_equivalence(
                    c,
                    node_removal_common::Method::ImplicitDynamic,
                    2,
                )
            }),
        ),
        (
            "node_removal_rollback_one_rank_refuses",
            Box::new(node_removal_common::rollback_when_one_rank_refuses),
        ),
        (
            "node_removal_inconsistent_element_erasure_is_refused",
            Box::new(node_removal_common::inconsistent_element_erasure_is_refused),
        ),
        // Element events
        // composed in one mutation refused before the commit on every rank;
        // the synchronizers after an erasure that moves elements
        (
            "node_removal_two_erasures_of_a_block_are_refused",
            Box::new(node_removal_common::two_erasures_of_a_block_are_refused),
        ),
        (
            "node_removal_added_then_erased_is_refused",
            Box::new(node_removal_common::added_then_erased_is_refused),
        ),
        (
            "node_removal_synchronizers_after_a_middle_erasure",
            Box::new(node_removal_common::synchronizers_after_a_middle_erasure),
        ),
        // MPI-041, MPI-053: one process per rank, so the random
        // `Yd` draws are each rank's own (upstream's per-process generator)
        (
            "marigo_distributed_vs_serial",
            Box::new(par_laws::marigo_distributed_vs_serial),
        ),
        (
            "clh_init_cl_exchange",
            Box::new(par_laws::clh_init_cl_exchange),
        ),
        (
            "extension_tag_dispatch",
            Box::new(par_laws::extension_tag_dispatch),
        ),
        (
            "phasefield_degrade_mass_lumped_vs_serial",
            Box::new(par_laws::phasefield_degrade_mass_lumped_vs_serial),
        ),
        (
            "law_data_for_a_generic_tag_is_refused",
            Box::new(par_laws::law_data_for_a_generic_tag_is_refused),
        ),
        (
            "smm_density_carries_damage_and_grad_u",
            Box::new(par_laws::smm_density_carries_damage_and_grad_u),
        ),
        // CL-008, MPI-034: two laws,
        // one in finite deformation
        (
            "two_law_ghosts_take_owner_law_and_internals",
            Box::new(par_smm::two_law_ghosts_take_owner_law_and_internals),
        ),
        (
            "explicit_two_laws_finite_deformation_distributed_vs_serial",
            Box::new(|c| {
                par_smm::distributed_vs_serial_laws(
                    c,
                    Run::Explicit,
                    4,
                    40,
                    Part::Rcb,
                    1e-12,
                    par_smm::Laws::TwoWithFiniteDeformation,
                )
            }),
        ),
        // upstream test_solid_mechanics_model_reassign_material (PARALLEL,
        // the `_N` variants: TEST-105 / SMM-020):
        // upstream's condition after initFull and after reassignMaterial on
        // every local and ghost element
        (
            "test_solid_mechanics_model_reassign_material_parallel",
            Box::new(|c| reassign_scenario::scenario(c, false)),
        ),
        // upstream test_dof_synchronizer (PARALLEL) on its bar.msh (MPI-018)
        (
            "test_dof_synchronizer_bar_msh",
            Box::new(|c| {
                let m = dof_sync_scenario::bar_msh(&c)?;
                dof_sync_scenario::check_dof_synchronizer(&c, &m)
            }),
        ),
        // TEST-109 `PARALLEL`: the implicit bar
        // wave on real MPI against upstream's criterion and the serial run
        (
            "smm_bar_dynamics_implicit_vs_serial",
            Box::new(smm_bar_dynamics_implicit_vs_serial),
        ),
    ];
    let size = control.size();
    // A scenario that does not run at this rank count is
    // not listed (no PASS, not counted) and says why
    let mut not_run = par_common::NotRun::default();
    let mut scenarios = scenarios;
    // rank-specific failures agreed on every rank: meaningful from 2 ranks
    // (at 1 rank the model has no element synchronizer and the failure is
    // local; the agreement itself is what these scenarios check)
    let multi: [Named; 9] = [
        (
            "clh_init_cl_failure_is_agreed_fatal",
            par_laws::clh_init_cl_failure_is_agreed_fatal,
        ),
        (
            "smm_density_failure_is_agreed",
            par_laws::smm_density_failure_is_agreed,
        ),
        (
            "pa01_rank_specific_selector_failure_is_agreed",
            par_smm::pa01_rank_specific_selector_failure_is_agreed,
        ),
        (
            "pa01_rank_specific_init_material_failure_is_agreed",
            par_smm::pa01_rank_specific_init_material_failure_is_agreed,
        ),
        (
            "pa2_01_law_id_exchange_failure_is_agreed_fatal",
            par_smm::pa2_01_law_id_exchange_failure_is_agreed_fatal,
        ),
        (
            "pa2_02_instantiation_failure_is_agreed_fatal",
            par_smm::pa2_02_instantiation_failure_is_agreed_fatal,
        ),
        // non-local collectives
        (
            "nl_init_failure_is_agreed",
            par_nl::nl_init_failure_is_agreed,
        ),
        (
            "nl_cycle_failure_is_agreed",
            par_nl::nl_cycle_failure_is_agreed,
        ),
        (
            "nl_max_criterion_failure_is_agreed",
            par_nl::max_criterion_failure_is_agreed,
        ),
    ];
    for (name, f) in multi {
        if size > 1 {
            scenarios.push((name, Box::new(f)));
        } else {
            not_run.add(name, "a rank-specific failure needs at least 2 ranks");
        }
    }
    // oracle differentials: (scenario, fixture it replays, function,
    // NativeMpi policy); each runs at every rank count the probe recorded
    // (fixtures/parallel/<fixture>/np<P>: 1, 2 and 4 processes, the static
    // one at 1 and 2, smm_explicit_cube_n4 also at 8)
    let oracle: Vec<(&str, &str, S, bool)> = vec![
        (
            "smm_explicit_matches_oracle",
            "smm_explicit_cube_n4",
            Box::new(par_oracle::explicit_matches),
            false,
        ),
        // NativeMpi policy: energies reduced by MPI_Allreduce as upstream
        (
            "smm_explicit_matches_oracle_native_mpi",
            "smm_explicit_cube_n4",
            Box::new(par_oracle::explicit_matches),
            true,
        ),
        // Two laws, one in finite deformation (both
        // reduction policies)
        (
            "smm_explicit_two_laws_matches_oracle",
            "smm_explicit_two_laws_cube_n4",
            Box::new(par_oracle::explicit_two_laws_matches),
            false,
        ),
        (
            "smm_explicit_two_laws_matches_oracle_native_mpi",
            "smm_explicit_two_laws_cube_n4",
            Box::new(par_oracle::explicit_two_laws_matches),
            true,
        ),
        (
            "smm_static_matches_oracle",
            "smm_static_cube_n3",
            Box::new(par_oracle::static_matches),
            false,
        ),
        // MPI-041: one process per rank, its own generator
        (
            "clh_marigo_matches_oracle",
            "clh_marigo_cube_n3",
            Box::new(par_laws::clh_marigo_matches_oracle),
            false,
        ),
    ];
    // MPI-048…050, TEST-079 / 080: non-local laws on
    // distributed meshes; each oracle replay runs at the rank counts the
    // probe recorded (and is not listed at the others)
    let nl_oracle: Vec<(&str, &str, S)> = vec![
        (
            "nl_region_matches_oracle",
            "nl_region",
            Box::new(par_nl::region_matches_oracle),
        ),
        (
            "nl_pair_matches_oracle",
            "nl_pair",
            Box::new(par_nl::pair_matches_oracle),
        ),
        (
            "nl_mazars_matches_oracle",
            "nl_mazars",
            Box::new(|c| par_nl::mazars_matches_oracle(c, "nl_mazars")),
        ),
        (
            "nl_mixed_matches_oracle",
            "nl_mixed",
            Box::new(|c| par_nl::mazars_matches_oracle(c, "nl_mixed")),
        ),
        (
            "nl_mazars_two_strips_matches_oracle",
            "nl_mazars_two_strips",
            Box::new(|c| par_nl::mazars_matches_oracle(c, "nl_mazars_two_strips")),
        ),
        (
            "nl_damage_wf_matches_oracle",
            "nl_damage_wf",
            Box::new(par_nl::damage_wf_matches_oracle),
        ),
    ];
    // Every listed fixture is recorded at some count
    let fixtures: Vec<&str> = oracle
        .iter()
        .map(|(_, f, _, _)| *f)
        .chain(nl_oracle.iter().map(|(_, f, _)| *f))
        .collect();
    if let Err(e) = par_common::missing_oracle_fixtures("parallel", &fixtures) {
        println!("FAIL start-up (rank {}): {e}", control.rank());
        drop(control);
        std::process::exit(1);
    }
    for (name, fixture, f) in nl_oracle {
        if par_common::oracle_rank_counts(fixture).contains(&size) {
            scenarios.push((name, f));
        } else {
            not_run.add(name, par_common::not_recorded("parallel", fixture, size));
        }
    }
    scenarios.push((
        "nl_region_default_vs_serial",
        Box::new(|c| par_nl::region_default_vs_serial(c, 1e-14)),
    ));
    scenarios.push((
        "test_build_neighborhood_parallel",
        Box::new(par_nl::test_build_neighborhood_parallel),
    ));
    scenarios.push((
        "test_pair_computation",
        Box::new(par_nl::test_pair_computation),
    ));
    scenarios.push((
        "nl_max_criterion_vs_serial",
        Box::new(par_nl::max_criterion_vs_serial),
    ));
    scenarios.push((
        "nl_damage_wf_exchange_ud1103",
        Box::new(par_nl::damage_wf_exchange),
    ));
    scenarios.push((
        "nl_mixed_cleanup_ud1101",
        Box::new(|c| par_nl::mixed_cleanup_ud1101(c, 1e-14)),
    ));
    // Multi-point elements, the UD-1100
    // compat outcome, two neighbourhoods over a damaging history
    scenarios.push((
        "nl_multi_point_default_vs_serial",
        Box::new(par_nl::v140::multi_point_default_vs_serial),
    ));
    scenarios.push((
        "nl_max_criterion_multi_point_vs_serial",
        Box::new(par_nl::max_criterion_multi_point_vs_serial),
    ));
    scenarios.push((
        "nl_ud1100_compat_outcome_is_agreed",
        Box::new(par_nl::v140::ud1100_compat_outcome_is_agreed),
    ));
    scenarios.push((
        "nl_two_neighbourhoods_default_vs_serial",
        Box::new(par_nl::v140::two_neighbourhoods_default_vs_serial),
    ));
    // D-097 (1): KeyOrder refused on a distributed
    // mesh, recoverably (then init_materials of the same model),
    // accepted at one rank on a mesh that is not distributed
    scenarios.push((
        "nl_key_order_refused_on_a_distributed_mesh",
        Box::new(par_nl::key_order_refused_on_a_distributed_mesh),
    ));
    let mut failed = 0u64;
    let mut count = scenarios.len();
    let native = world_with(ReductionPolicy::NativeMpi);
    let mut all: Vec<(&str, &S, bool)> = scenarios.iter().map(|(n, f)| (*n, f, false)).collect();
    for (n, fixture, f, nat) in &oracle {
        if par_common::oracle_rank_counts(fixture).contains(&size) {
            all.push((n, f, *nat));
            count += 1;
        } else {
            not_run.add(*n, par_common::not_recorded("parallel", fixture, size));
        }
    }
    not_run.print(control.rank());
    for (name, f, nat) in all {
        let comm = if nat {
            native.duplicate()
        } else {
            control.duplicate()
        }
        .expect("duplicate");
        let t0 = std::time::Instant::now();
        let r = f(comm);
        let mut bad = [u64::from(r.is_err())];
        control
            .all_reduce(&mut bad, ReduceOp::Sum)
            .expect("all_reduce");
        if let Err(e) = &r {
            println!("FAIL {name} (rank {}): {e}", control.rank());
        }
        if bad[0] > 0 {
            failed += 1;
        } else if control.rank() == 0 {
            println!("PASS {name} ({:.2} s)", t0.elapsed().as_secs_f64());
        }
    }
    if control.rank() == 0 {
        println!(
            "SUMMARY ranks={} scenarios={} failed={failed} not_run={}",
            control.size(),
            count,
            not_run.count()
        );
    }
    drop(native);
    drop(control);
    std::process::exit(i32::from(failed > 0));
}
