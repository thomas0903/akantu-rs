// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/{material_orthotropic_damage_iterative.cc,
// material_damage_iterative.cc, material_iterative_stiffness_reduction.cc}
// (updateDamage).
//! Regressions: `updateDamage` of the iterative damage
//! laws at 2 ranks when one rank fails locally before the count reduction
//! (upstream `comm.allReduce(nb_damaged, _sum)`). Before the fix the
//! failing rank returned at once while the other waited in the raw `Sum`
//! all-reduce until the local world's timeout (under MPI: forever). Now
//! the local outcome is agreed with the reduction: **every** rank returns
//! an error well before the timeout, and no rank changes its damage state.
//!
//! * `orthotropic_damage_iterative`, UD-870 compat
//!   (`orthotropic_damage_global_maximum_only = false`): rank 1 holds no
//!   maximum point (all its points compressive) while the global maximum is
//!   1.5: rank 1 is refused ("no point holds the maximum").
//! * `orthotropic_damage_iterative`, default mode: the damaging rank (0)
//!   fails in the eigen-decomposition (NaN stress direction at its maximum
//!   point, injected); the `Min` step of UD-870 runs in the same scope.
//! * `damage_iterative` (saw-tooth): `prescribed_dam ≤ 0` on rank 1 only.
//! * `iterative_stiffness_reduction`: a truncated `eps_u` block on rank 1.
//!
//! The state is set by hand on the real code path: local equivalent
//! stresses, the law's own
//! `find_max_normalized_equivalent_stress`, its declared reduction
//! (`reduce_after_stresses`, through a reducer issuing the all-reduce: no
//! model operation is running), then `update_damage` as the upstream
//! program calls it after a solve. A control without injection damages the
//! expected points on every rank.

mod common;

use std::time::{Duration, Instant};

use akantu_comm::{Communicator, ReduceOp, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType, SparseSolverType};
use akantu_core::{ElementType, GhostType, Real};
use akantu_extra_materials::orthotropic_damage::MaterialOrthotropicDamageIterative;
use akantu_extra_materials::{
    ExtraLawOptions, MaterialDamageIterative, MaterialIterativeStiffnessReduction,
    as_iterative_damage_mut, register_extra_materials_with,
};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{LawPhase, LawReducer};
use akantu_parser::Parser;
use akantu_solid::{Material, SolidMechanicsModel};
use common::data;

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;
/// Local-world receive timeout: a rank left waiting would report it.
const TIMEOUT: Duration = Duration::from_secs(15);
/// Every rank must return well before the timeout.
const PROMPT: f64 = 5.0;

const ORTHOTROPIC: &str = "material orthotropic_damage_iterative [\n name = m\n rho = 1\n E = 1\n \
                           nu = 0\n Sc = 10\n prescribed_dam = 0.1\n]\n";
const SAWTOOTH: &str = "material damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n Sc = \
                        10\n prescribed_dam = 0.1\n max_damage = 0.99\n]\n";
const STIFFNESS: &str = "material iterative_stiffness_reduction [\n name = m\n rho = 1\n E = 1\n \
                         nu = 0\n Sc = 10\n prescribed_dam = 0.1\n max_damage = 0.9999\n Gf = \
                         160\n crack_band_width = 1\n max_reductions = 10\n]\n";

struct RawReducer(Communicator);

impl LawReducer for RawReducer {
    fn nb_ranks(&self) -> usize {
        self.0.size()
    }
    fn all_reduce_real(&mut self, v: &mut [Real], op: ReduceOp) -> akantu_core::Result<()> {
        self.0.all_reduce(v, op)
    }
}

fn model_on(comm: &Communicator, dat: &str, options: ExtraLawOptions) -> SolidMechanicsModel {
    let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).unwrap();
    if comm.rank() == 0 {
        akantu_io::read_mesh(
            &mut mesh,
            &data("test_material_damage/plate.msh"),
            MeshIoType::Gmsh,
        )
        .unwrap();
    }
    let mut p = RcbPartitioner;
    let _ = distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    register_extra_materials_with(&mut model.physics.factory, options).unwrap();
    let mut o = ModelOptions::new(AnalysisMethod::Static);
    o.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    model.init_full(&o).unwrap();
    model
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Case {
    /// Orthotropic, UD-870 compat, rank 1 without a maximum point.
    OrthotropicCompatNoMaximum,
    /// Orthotropic, default mode, eigen failure on the damaging rank 0.
    OrthotropicDefaultEigenFailure,
    /// Saw-tooth, `prescribed_dam = -1` on rank 1.
    SawtoothBadParameter,
    /// Stiffness reduction, truncated `eps_u` block on rank 1.
    StiffnessTruncatedBlock,
    /// Control (no injection): saw-tooth.
    SawtoothControl,
    /// Control (no injection): orthotropic default mode.
    OrthotropicControl,
}

impl Case {
    fn dat(self) -> &'static str {
        match self {
            Case::OrthotropicCompatNoMaximum
            | Case::OrthotropicDefaultEigenFailure
            | Case::OrthotropicControl => ORTHOTROPIC,
            Case::SawtoothBadParameter | Case::SawtoothControl => SAWTOOTH,
            Case::StiffnessTruncatedBlock => STIFFNESS,
        }
    }
    fn options(self) -> ExtraLawOptions {
        ExtraLawOptions {
            orthotropic_damage_global_maximum_only: self != Case::OrthotropicCompatNoMaximum,
            ..ExtraLawOptions::default()
        }
    }
    fn fails(self) -> bool {
        !matches!(self, Case::SawtoothControl | Case::OrthotropicControl)
    }
}

/// Normalised equivalent stress of point `e` on `rank`: the global maximum
/// 1.5 at rank 0's first point; rank 1 compressive in the orthotropic
/// compat case (no maximum point there), else 0.1.
fn equivalent_stress(case: Case, rank: akantu_core::Rank, e: usize) -> Real {
    match (rank, e) {
        (0, 0) => 1.5,
        (0, _) => 0.1,
        _ if case == Case::OrthotropicCompatNoMaximum => -1.0,
        _ => 0.1,
    }
}

/// The damage state the update may change (bits), per law.
fn state(law: &dyn Material) -> Vec<u64> {
    let any = law.as_any().unwrap();
    let mut out: Vec<u64> = Vec::new();
    if let Some(m) = any.downcast_ref::<MaterialOrthotropicDamageIterative<2>>() {
        out.extend(
            m.damage
                .get(T3, NG)
                .unwrap()
                .as_slice()
                .iter()
                .map(|x| x.to_bits()),
        );
        out.extend(
            m.damage_dir_vecs
                .get(T3, NG)
                .unwrap()
                .as_slice()
                .iter()
                .map(|x| x.to_bits()),
        );
    } else {
        let p = any
            .downcast_ref::<MaterialDamageIterative<2>>()
            .or_else(|| {
                any.downcast_ref::<MaterialIterativeStiffnessReduction<2>>()
                    .map(|s| &s.parent)
            })
            .unwrap();
        out.extend(
            p.damage
                .damage
                .get(T3, NG)
                .unwrap()
                .as_slice()
                .iter()
                .map(|x| x.to_bits()),
        );
        out.extend(
            p.reduction_step
                .get(T3, NG)
                .unwrap()
                .as_slice()
                .iter()
                .map(|&x| u64::from(x)),
        );
        out.extend(
            p.sc.get(T3, NG)
                .unwrap()
                .as_slice()
                .iter()
                .map(|x| x.to_bits()),
        );
    }
    out
}

/// Set the stresses, inject the case's local failure, reduce the maximum.
fn prepare(law: &mut dyn Material, case: Case, comm: &Communicator) -> akantu_core::Result<()> {
    let rank = comm.rank();
    let any = law.as_any_mut().unwrap();
    if let Some(m) = any.downcast_mut::<MaterialOrthotropicDamageIterative<2>>() {
        m.reset_norm_max_equivalent_stress();
        for (e, v) in m
            .equivalent_stress
            .get_mut(T3, NG)?
            .as_mut_slice()
            .iter_mut()
            .enumerate()
        {
            *v = equivalent_stress(case, rank, e);
        }
        m.find_max_normalized_equivalent_stress(T3, NG)?;
        if case == Case::OrthotropicDefaultEigenFailure && rank == 0 {
            m.stress_dir.get_mut(T3, NG)?.as_mut_slice()[..4].fill(Real::NAN);
        }
    } else {
        let stiffness = any.is::<MaterialIterativeStiffnessReduction<2>>();
        if stiffness {
            let s = any
                .downcast_mut::<MaterialIterativeStiffnessReduction<2>>()
                .unwrap();
            if case == Case::StiffnessTruncatedBlock && rank == 1 {
                s.eps_u.get_mut(T3, NG)?.truncate(0);
            }
        }
        let p = if stiffness {
            &mut any
                .downcast_mut::<MaterialIterativeStiffnessReduction<2>>()
                .unwrap()
                .parent
        } else {
            any.downcast_mut::<MaterialDamageIterative<2>>().unwrap()
        };
        p.reset_norm_max_equivalent_stress();
        for (e, v) in p
            .equivalent_stress
            .get_mut(T3, NG)?
            .as_mut_slice()
            .iter_mut()
            .enumerate()
        {
            *v = equivalent_stress(case, rank, e);
        }
        p.find_max_normalized_equivalent_stress(T3, NG)?;
        if case == Case::SawtoothBadParameter && rank == 1 {
            p.prescribed_dam = -1.0;
        }
    }
    law.reduce_after_stresses(NG, &mut RawReducer(comm.clone()))
}

struct Outcome {
    seconds: f64,
    result: Result<u32, String>,
    unchanged: bool,
}

fn scenario(comm: Communicator, case: Case) -> Outcome {
    let mut model = model_on(&comm, case.dat(), case.options());
    let c = comm.clone();
    let before = model
        .with_material(0, LawPhase::Energy, move |law, _| {
            prepare(law, case, &c)?;
            Ok(state(law))
        })
        .unwrap();
    let t0 = Instant::now();
    let r = model.with_material(0, LawPhase::AfterSolveStep, |law, ctx| {
        as_iterative_damage_mut(law).unwrap().update_damage(ctx)
    });
    let seconds = t0.elapsed().as_secs_f64();
    let after = model
        .with_material(0, LawPhase::Energy, |law, _| Ok(state(law)))
        .unwrap();
    eprintln!(
        "{case:?}: rank {}: update_damage after {seconds:.2} s: {r:?}",
        comm.rank()
    );
    Outcome {
        seconds,
        result: r.map_err(|e| e.to_string()),
        unchanged: before == after,
    }
}

/// Every rank refused promptly, the failing rank with its own error, the
/// others with the agreed "failed on another rank"; no state changed.
fn assert_agreed_refusal(case: Case, out: &[Outcome], own: &str, failing_rank: usize) {
    for (r, o) in out.iter().enumerate() {
        assert!(
            o.seconds < PROMPT,
            "{case:?}: rank {r} returned after {:.2} s",
            o.seconds
        );
        let err = o
            .result
            .as_ref()
            .expect_err(&format!("{case:?}: rank {r} succeeded"));
        if r == failing_rank {
            assert!(err.contains(own), "{case:?}: rank {r}: {err}");
        } else {
            assert!(
                err.contains("failed on another rank"),
                "{case:?}: rank {r}: {err}"
            );
            assert!(err.contains("update_damage"), "{case:?}: rank {r}: {err}");
        }
        assert!(o.unchanged, "{case:?}: rank {r} changed its damage state");
    }
}

#[test]
fn orthotropic_compat_rank_without_maximum_is_an_agreed_refusal() {
    let case = Case::OrthotropicCompatNoMaximum;
    let out = run_local(2, TIMEOUT, move |c| scenario(c, case));
    assert_agreed_refusal(case, &out, "no point holds the maximum", 1);
}

#[test]
fn orthotropic_eigen_failure_on_the_damaging_rank_is_an_agreed_refusal() {
    let case = Case::OrthotropicDefaultEigenFailure;
    let out = run_local(2, TIMEOUT, move |c| scenario(c, case));
    assert_agreed_refusal(case, &out, "non-finite eigenvalue", 0);
}

#[test]
fn sawtooth_failure_on_one_rank_is_an_agreed_refusal() {
    let case = Case::SawtoothBadParameter;
    let out = run_local(2, TIMEOUT, move |c| scenario(c, case));
    assert_agreed_refusal(case, &out, "prescribed damage must be greater than zero", 1);
}

#[test]
fn stiffness_reduction_failure_on_one_rank_is_an_agreed_refusal() {
    let case = Case::StiffnessTruncatedBlock;
    let out = run_local(2, TIMEOUT, move |c| scenario(c, case));
    assert_agreed_refusal(case, &out, "updateDamage", 1);
}

/// Controls: without injection the update succeeds on every rank with the
/// same summed count (saw-tooth: rank 0's maximum point only, the others
/// are below `(1 − dam_tolerance)·1.5`; orthotropic default: one point),
/// and only rank 0 changes its state.
#[test]
fn controls_damage_the_expected_points_on_every_rank() {
    for case in [Case::SawtoothControl, Case::OrthotropicControl] {
        assert!(!case.fails());
        let out = run_local(2, TIMEOUT, move |c| scenario(c, case));
        for (r, o) in out.iter().enumerate() {
            assert_eq!(o.result, Ok(1), "{case:?}: rank {r}");
            assert_eq!(
                o.unchanged,
                r != 0,
                "{case:?}: rank {r}: state changed = {}",
                !o.unchanged
            );
        }
    }
}
