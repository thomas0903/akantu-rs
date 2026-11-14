// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/test/
// test_material_damage/{test_material_damage_iterative.cc,
// test_material_damage_iterative_parallel.sh (`mpirun -np 2`), material.dat,
// material_stiffness_reduction.dat, plate.msh}.

//! Parallel runs of the iterative-damage laws (XMAT-001 /
//! 003 / 010, TEST-184 / 185) on a real multi-rank communicator (local world
//! at 2 and 4 ranks, rank 0 reads the plate, RCB partition, as upstream's
//! `mesh.distribute()`):
//!
//! * `updateDamage()` alone: the `Sum` all-reduce of the
//!   damaged count, with the state of a solve set by hand;
//! * whole solves: the `Max` all-reduce of
//!   `computeAllStresses` runs inside the solver's residual assembly, as a
//!   reduction the law declares and the model issues through the solver's
//!   collective scope; upstream's TEST-184 program as written (each rank
//!   weakens its local elements 1 and 5: `2·psize` damaged elements), and a
//!   case that only the global maximum gets right (two weakened elements on
//!   different ranks, the less weak one below the damage threshold of the
//!   global maximum but above that of its own rank's maximum): the serial
//!   pattern, count and maximum at 1, 2 and 4 ranks for `damage_iterative`,
//!   `iterative_stiffness_reduction` and `orthotropic_damage_iterative`
//!   (UD-870: upstream damages one point per rank there).

mod common;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{AnalysisMethod, MeshIoType, SparseSolverType};
use akantu_core::{ElementType, GhostType};
use akantu_extra_materials::{
    ExtraLawOptions, MaterialDamageIterative, MaterialIterativeStiffnessReduction,
    register_extra_materials_with,
};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use common::{data, with_iterative, with_law};

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;
const TIMEOUT: Duration = Duration::from_secs(300);

type Outcome = Result<(), String>;

/// Rank 0 reads `msh`, the mesh is distributed with the RCB partitioner
/// (upstream: `if (prank == 0) mesh.read(...); mesh.distribute();`), the
/// model is built from the input `dat` and initialised for a static solve.
fn static_model(comm: &Communicator, msh: &str, dat: &str) -> SolidMechanicsModel {
    let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).unwrap();
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut mesh, &data(msh), MeshIoType::Gmsh).unwrap();
    }
    if comm.size() > 1 {
        let mut p = RcbPartitioner;
        let _events =
            distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
    }
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    register_extra_materials_with(&mut model.physics.factory, ExtraLawOptions::default()).unwrap();
    let mut options = ModelOptions::new(AnalysisMethod::Static);
    if comm.size() > 1 {
        // the Eigen backend refuses parallel systems (as upstream); the
        // faer extension solves the centralised system on rank 0 (D-040)
        options.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    model.init_full(&options).unwrap();
    model
}

/// The `damage_iterative` core of law 0 (the law itself or the parent of
/// `iterative_stiffness_reduction`).
fn with_core<R>(
    model: &mut SolidMechanicsModel,
    f: impl FnOnce(&mut MaterialDamageIterative<2>) -> R,
) -> R {
    let mut f = Some(f);
    let mut out = None;
    with_law(model, 0, |law, _| {
        let any = law.as_any_mut().unwrap();
        let core = if any.is::<MaterialIterativeStiffnessReduction<2>>() {
            &mut any
                .downcast_mut::<MaterialIterativeStiffnessReduction<2>>()
                .unwrap()
                .parent
        } else {
            any.downcast_mut::<MaterialDamageIterative<2>>().unwrap()
        };
        out = Some((f.take().unwrap())(core));
        Ok(())
    });
    out.unwrap()
}

/// The elements `e < rank + 1` of a rank carry the normalised equivalent
/// stress `1`, the others `0.5`; the global maximum is `1` (what a solve's
/// `Max` all-reduce gives). `update_damage` then damages `rank + 1` elements
/// per rank and returns the `Sum` over the ranks: `n (n + 1) / 2`.
fn parallel_update_damage(comm: Communicator, dat: &str, damage_of_one_step: f64) -> Outcome {
    let (n, rank) = (comm.size(), usize::try_from(comm.rank()).unwrap());
    let mut model = static_model(&comm, "test_material_damage/plate.msh", dat);
    let nb = with_core(&mut model, |m| {
        let mut eq = m.equivalent_stress.get_mut(T3, NG).unwrap();
        let nb = eq.len();
        for (e, v) in eq.as_mut_slice().iter_mut().enumerate() {
            *v = if e <= rank { 1. } else { 0.5 };
        }
        drop(eq);
        *m.norm_max_equivalent_stress_mut() = 1.;
        nb
    });
    if nb <= n {
        return Err(format!("rank {rank}: only {nb} elements"));
    }
    let count = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
    let want = u32::try_from(n * (n + 1) / 2).unwrap();
    if count != want {
        return Err(format!("rank {rank}: {count} damaged, expected {want}"));
    }
    // each rank damaged exactly its own elements 0..=rank
    let damage = with_core(&mut model, |m| {
        m.damage.damage.get(T3, NG).unwrap().as_slice().to_vec()
    });
    for (e, &d) in damage.iter().enumerate() {
        let want = if e <= rank { damage_of_one_step } else { 0. };
        if (d - want).abs() > 1e-13 {
            return Err(format!("rank {rank}: damage[{e}] = {d}, expected {want}"));
        }
    }
    // a second call: the maximum is still 1, the already damaged points
    // (equivalent stress 1) are damaged once more: the count is the same
    let again = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
    if again != want {
        return Err(format!("rank {rank}: second call {again}, expected {want}"));
    }
    Ok(())
}

fn on_ranks(dat: &'static str, d: f64) {
    for n in [2, 4] {
        let out = run_local(n, TIMEOUT, |c| parallel_update_damage(c, dat, d));
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

/// XMAT-001 / TEST-185: `damage_iterative` (`prescribed_dam = 0.1`).
#[test]
fn damage_iterative_update_damage_sums_over_the_ranks() {
    on_ranks(
        "material damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n Sc = 10\n \
         prescribed_dam = 0.1\n max_damage = 0.99\n]\n",
        0.1,
    );
}

/// XMAT-003: `iterative_stiffness_reduction` (the first reduction of a point
/// gives `d = 1 − 1/reduction_constant`, default constant 2: `0.5`).
#[test]
fn iterative_stiffness_reduction_update_damage_sums_over_the_ranks() {
    on_ranks(
        "material iterative_stiffness_reduction [\n name = m\n rho = 1\n E = 60e9\n nu = 0.3\n \
         Sc = 5.7e6\n prescribed_dam = 0.1\n max_damage = 0.9999\n Gf = 160\n \
         crack_band_width = 1\n max_reductions = 10\n]\n",
        0.5,
    );
}

// ---------------------------------------------------------------------------
// Whole solves at 2 / 4 ranks
// ---------------------------------------------------------------------------

use akantu_comm::ReduceOp;
use akantu_core::enums::SpatialDirection;
use akantu_core::math::are_float_equal;
use akantu_core::{Real, UInt};
use akantu_extra_materials::orthotropic_damage::MaterialOrthotropicDamageIterative;
use akantu_extra_materials::{ExtraLawOptions as Opts, as_iterative_damage};
use akantu_model::boundary_condition::FixedValue;

/// `Math::setTolerance(1e-13)` of the upstream program.
const TOLERANCE: Real = 1e-13;

/// Upstream's Dirichlet conditions (left `u_x = 0`, bottom `u_y = 0`, top
/// `u_y = 2`): uniaxial tension, `σ_yy = 1` with `E = 1`, `ν = 0`.
fn tension(model: &mut SolidMechanicsModel) -> Result<(), String> {
    for (value, axis, group) in [
        (0., SpatialDirection::X, "left"),
        (0., SpatialDirection::Y, "bottom"),
        (2., SpatialDirection::Y, "top"),
    ] {
        model
            .apply_dirichlet_bc(&FixedValue { value, axis }, group)
            .map_err(|e| format!("{group}: {e}"))?;
    }
    Ok(())
}

/// `static_model` with explicit extra-law switches.
fn static_model_with(comm: &Communicator, dat: &str, options: Opts) -> SolidMechanicsModel {
    let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).unwrap();
    if comm.rank() == 0 {
        akantu_io::read_mesh(
            &mut mesh,
            &data("test_material_damage/plate.msh"),
            MeshIoType::Gmsh,
        )
        .unwrap();
    }
    if comm.size() > 1 {
        let mut p = RcbPartitioner;
        let _events =
            distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
    }
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser).unwrap();
    register_extra_materials_with(&mut model.physics.factory, options).unwrap();
    let mut o = ModelOptions::new(AnalysisMethod::Static);
    if comm.size() > 1 {
        o.solver_options.sparse_solver_type = Some(SparseSolverType::Faer);
    }
    model.init_full(&o).unwrap();
    model
}

/// The three laws under test, by their state needed here.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Law {
    DamageIterative,
    StiffnessReduction,
    Orthotropic,
}

impl Law {
    fn dat(self) -> &'static str {
        match self {
            Law::DamageIterative => {
                "material damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n Sc = 10\n \
                 prescribed_dam = 0.1\n max_damage = 0.99\n]\n"
            }
            Law::StiffnessReduction => {
                "material iterative_stiffness_reduction [\n name = m\n rho = 1\n E = 1\n nu = 0\n \
                 Sc = 10\n prescribed_dam = 0.1\n max_damage = 0.9999\n Gf = 160\n \
                 crack_band_width = 1\n max_reductions = 10\n]\n"
            }
            Law::Orthotropic => {
                "material orthotropic_damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n \
                 Sc = 10\n prescribed_dam = 0.1\n]\n"
            }
        }
    }

    /// Per element (one point per `_triangle_3`): normalised equivalent
    /// stress, and the damage measure (scalar damage; the trace of the
    /// orthotropic damage tensor).
    fn state(self, model: &mut SolidMechanicsModel) -> (Vec<Real>, Vec<Real>) {
        if self == Law::Orthotropic {
            return with_law(model, 0, |law, _| {
                let m = law
                    .as_any()
                    .unwrap()
                    .downcast_ref::<MaterialOrthotropicDamageIterative<2>>()
                    .unwrap();
                let eq = m.equivalent_stress.get(T3, NG)?.as_slice().to_vec();
                let d = m.damage.get(T3, NG)?;
                let tr = d
                    .as_slice()
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|t| t[0] + t[3])
                    .collect();
                Ok((eq, tr))
            });
        }
        with_core(model, |m| {
            (
                m.equivalent_stress.get(T3, NG).unwrap().as_slice().to_vec(),
                m.damage.damage.get(T3, NG).unwrap().as_slice().to_vec(),
            )
        })
    }

    fn set_sc(self, model: &mut SolidMechanicsModel, e: usize, sc: Real) {
        if self == Law::Orthotropic {
            with_law(model, 0, |law, _| {
                let m = law
                    .as_any_mut()
                    .unwrap()
                    .downcast_mut::<MaterialOrthotropicDamageIterative<2>>()
                    .unwrap();
                m.sc.get_mut(T3, NG)?.as_mut_slice()[e] = sc;
                Ok(())
            });
            return;
        }
        with_core(model, |m| {
            m.sc.get_mut(T3, NG).unwrap().as_mut_slice()[e] = sc
        });
    }

    /// Damage of the first `updateDamage` of a point.
    fn first_damage(self) -> Real {
        match self {
            Law::DamageIterative | Law::Orthotropic => 0.1,
            // 1 − 1/reduction_constant (default 2)
            Law::StiffnessReduction => 0.5,
        }
    }
}

fn norm_max(model: &SolidMechanicsModel) -> Real {
    as_iterative_damage(model.material(0).unwrap())
        .unwrap()
        .norm_max_equivalent_stress()
}

/// `(min, max)` of `x` over the ranks (raw collectives: no model operation
/// is running).
fn min_max(comm: &Communicator, x: Real) -> (Real, Real) {
    let (mut lo, mut hi) = ([x], [x]);
    comm.all_reduce(&mut lo, ReduceOp::Min).unwrap();
    comm.all_reduce(&mut hi, ReduceOp::Max).unwrap();
    (lo[0], hi[0])
}

/// Upstream's `test_material_damage_iterative` (TEST-184; its
/// `_parallel` variant is the same program under `mpirun -np 2`) on `comm`
/// with upstream's acceptance criteria: after the first solve every
/// normalised equivalent stress is 0.1 and nothing is damaged; each rank
/// weakens its local elements 1 and 5 (`Sc = 0.9, 0.898`), the second
/// solve damages `2·psize` elements, exactly the local elements 1 and 5 of
/// every rank (damage 0.1); a third solve. In addition the maximum is the
/// same on every rank after each solve (the `Max` all-reduce of
/// `computeAllStresses`).
fn upstream_program(comm: Communicator) -> Outcome {
    let (psize, rank) = (comm.size(), comm.rank());
    let dat = std::fs::read_to_string(data("test_material_damage/material.dat")).unwrap();
    let mut model = static_model_with(&comm, &dat, Opts::default());
    tension(&mut model)?;
    model
        .solve_step(None)
        .map_err(|e| format!("rank {rank}: solve 1: {e}"))?;
    let law = Law::DamageIterative;
    let (eq, _) = law.state(&mut model);
    if eq.len() < 6 {
        return Err(format!("rank {rank}: {} local elements", eq.len()));
    }
    for (e, &v) in eq.iter().enumerate() {
        if !are_float_equal(v, 0.1, TOLERANCE) {
            return Err(format!("rank {rank}: equivalent stress [{e}] = {v}"));
        }
    }
    let max = norm_max(&model);
    let (lo, hi) = min_max(&comm, max);
    if lo.to_bits() != hi.to_bits() {
        return Err(format!(
            "rank {rank}: maxima differ over the ranks: {lo} vs {hi}"
        ));
    }
    if max > 1. {
        return Err(format!(
            "rank {rank}: maximum {max} > 1 before the weakening"
        ));
    }
    law.set_sc(&mut model, 1, 0.9);
    law.set_sc(&mut model, 5, 0.898);
    model
        .solve_step(None)
        .map_err(|e| format!("rank {rank}: solve 2: {e}"))?;
    let max = norm_max(&model);
    let (lo, hi) = min_max(&comm, max);
    if lo.to_bits() != hi.to_bits() {
        return Err(format!(
            "rank {rank}: maxima differ over the ranks: {lo} vs {hi}"
        ));
    }
    let mut nb = 0;
    if max > 1. {
        nb = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
    }
    let want = u32::try_from(2 * psize).unwrap();
    if nb != want {
        return Err(format!(
            "rank {rank}: {nb} damaged elements, expected {want}"
        ));
    }
    let (_, damage) = law.state(&mut model);
    let mut diff = 0.;
    for (e, &d) in damage.iter().enumerate() {
        diff += if e == 1 || e == 5 { (0.1 - d).abs() } else { d };
    }
    if diff > 1e-13 {
        return Err(format!("rank {rank}: error in the damage pattern: {diff}"));
    }
    model
        .solve_step(None)
        .map_err(|e| format!("rank {rank}: solve 3: {e}"))?;
    Ok(())
}

/// TEST-184 / 185 (XMAT-001): upstream's program at 2 and 4 ranks.
#[test]
fn test_material_damage_iterative_parallel_program() {
    for n in [2, 4] {
        for (r, o) in run_local(n, TIMEOUT, upstream_program)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

/// The element of the serial plate whose nodes are exactly `want`
/// (coordinates are copied unchanged by the distribution).
fn element_with_nodes(mesh: &Mesh, want: &[[u64; 2]; 3]) -> Option<usize> {
    let x = mesh.nodes();
    let conn = mesh.connectivity(T3, NG).ok()?;
    conn.as_slice().as_chunks::<3>().0.iter().position(|c| {
        let mut k: Vec<[u64; 2]> = c
            .iter()
            .map(|&n| {
                let n = usize::try_from(n).unwrap();
                [
                    x.as_slice()[2 * n].to_bits(),
                    x.as_slice()[2 * n + 1].to_bits(),
                ]
            })
            .collect();
        k.sort_unstable();
        k == want
    })
}

/// The node coordinates (bits, sorted) of the serial elements nearest to
/// the bottom-left and to the top-right corner of the plate.
fn corner_elements() -> [[[u64; 2]; 3]; 2] {
    let mut mesh = Mesh::new(2, "serial").unwrap();
    akantu_io::read_mesh(
        &mut mesh,
        &data("test_material_damage/plate.msh"),
        MeshIoType::Gmsh,
    )
    .unwrap();
    let x = mesh.nodes().as_slice().to_vec();
    let conn = mesh.connectivity(T3, NG).unwrap().as_slice().to_vec();
    let key = |c: &[i32]| {
        let mut k: Vec<[u64; 2]> = c
            .iter()
            .map(|&n| {
                let n = usize::try_from(n).unwrap();
                [x[2 * n].to_bits(), x[2 * n + 1].to_bits()]
            })
            .collect();
        k.sort_unstable();
        [k[0], k[1], k[2]]
    };
    let centre = |c: &[i32]| {
        let p = |n: i32| usize::try_from(n).unwrap();
        (
            (x[2 * p(c[0])] + x[2 * p(c[1])] + x[2 * p(c[2])]) / 3.,
            (x[2 * p(c[0]) + 1] + x[2 * p(c[1]) + 1] + x[2 * p(c[2]) + 1]) / 3.,
        )
    };
    let nearest = |tx: Real, ty: Real| {
        let c = conn
            .as_chunks::<3>()
            .0
            .iter()
            .min_by(|a, b| {
                let (da, db) = (centre(&a[..]), centre(&b[..]));
                let fa = (da.0 - tx).powi(2) + (da.1 - ty).powi(2);
                let fb = (db.0 - tx).powi(2) + (db.1 - ty).powi(2);
                fa.total_cmp(&fb)
            })
            .unwrap();
        key(c)
    };
    [nearest(-1., -1.), nearest(1., 1.)]
}

/// The weakened strengths: `A` (bottom-left) reaches `1/0.9 = 1.11`, `B`
/// (top-right) `1/0.8 = 1.25`; damage threshold of the global maximum
/// `0.99 · 1.25 = 1.2375`: only `B` is damaged. A rank that saw only its
/// own maximum `1.11` (no `Max` all-reduce) would damage `A` too.
const SC_A: Real = 0.9;
const SC_B: Real = 0.8;

/// Damage pattern, count and maximum of one `updateDamage` after a solve
/// with `A` and `B` weakened, on `comm`: `Ok((count, damaged, owner of B))`
/// where `damaged` = this rank holds a damaged element and it is `B`.
fn global_maximum_case(
    comm: Communicator,
    law: Law,
    options: Opts,
    corners: [[[u64; 2]; 3]; 2],
) -> Result<(u32, bool, bool), String> {
    let rank = comm.rank();
    let mut model = static_model_with(&comm, law.dat(), options);
    tension(&mut model)?;
    let (a, b) = {
        let mesh = model.physics.base.mesh.read().unwrap();
        (
            element_with_nodes(&mesh, &corners[0]),
            element_with_nodes(&mesh, &corners[1]),
        )
    };
    if let Some(e) = a {
        law.set_sc(&mut model, e, SC_A);
    }
    if let Some(e) = b {
        law.set_sc(&mut model, e, SC_B);
    }
    // the two weakened elements are owned by exactly one rank each, and by
    // different ranks when there are several
    let mut owners = [UInt::from(a.is_some()), UInt::from(b.is_some()), 0];
    owners[2] = UInt::from(a.is_some() && b.is_some());
    comm.all_reduce(&mut owners, ReduceOp::Sum).unwrap();
    if owners[..2] != [1, 1] || (comm.size() > 1 && owners[2] != 0) {
        return Err(format!("rank {rank}: owners of A, B, both: {owners:?}"));
    }
    model
        .solve_step(None)
        .map_err(|e| format!("rank {rank}: solve: {e}"))?;
    let (eq, _) = law.state(&mut model);
    let max = norm_max(&model);
    // every rank holds the same maximum, bit for bit the owner's value of B
    let (lo, hi) = min_max(&comm, max);
    let eq_b = b.map_or(Real::NEG_INFINITY, |e| eq[e]);
    let (_, want) = min_max(&comm, eq_b);
    if lo.to_bits() != hi.to_bits() || max.to_bits() != want.to_bits() {
        return Err(format!(
            "rank {rank}: maximum {max} (ranks {lo}..{hi}), eq(B) = {want}"
        ));
    }
    if !are_float_equal(max, 0.1 * 10. / SC_B, TOLERANCE) {
        return Err(format!("rank {rank}: maximum {max}, expected 1/{SC_B}"));
    }
    let count = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
    let (_, damage) = law.state(&mut model);
    let damaged: Vec<usize> = (0..damage.len()).filter(|&e| damage[e] != 0.).collect();
    let d1 = law.first_damage();
    let only_b = match (b, damaged.as_slice()) {
        (_, []) => false,
        (Some(e), [k]) if *k == e => {
            if (damage[e] - d1).abs() > 1e-13 {
                return Err(format!("rank {rank}: damage of B {}", damage[e]));
            }
            true
        }
        _ => {
            return Err(format!(
                "rank {rank}: damaged elements {damaged:?} (A = {a:?}, B = {b:?})"
            ));
        }
    };
    Ok((count, only_b, b.is_some()))
}

/// The serial pattern at 1, 2 and 4 ranks: one damaged element (`B`) on
/// its owner, nothing elsewhere, the count 1 on every rank.
fn serial_pattern_on_ranks(law: Law) {
    let corners = corner_elements();
    for n in [1, 2, 4] {
        let out = run_local(n, TIMEOUT, move |c| {
            global_maximum_case(c, law, Opts::default(), corners)
        });
        for (r, o) in out.into_iter().enumerate() {
            match o {
                Ok((count, only_b, owns_b)) => {
                    assert_eq!(count, 1, "{law:?}, {n} ranks, rank {r}: count");
                    assert_eq!(only_b, owns_b, "{law:?}, {n} ranks, rank {r}: pattern");
                }
                Err(e) => panic!("{law:?}, {n} ranks, rank {r}: {e}"),
            }
        }
    }
}

/// XMAT-001 / TEST-185: `damage_iterative` in whole solves.
#[test]
fn damage_iterative_solves_keep_the_serial_pattern_on_2_and_4_ranks() {
    serial_pattern_on_ranks(Law::DamageIterative);
}

/// XMAT-003: `iterative_stiffness_reduction` in whole solves.
#[test]
fn iterative_stiffness_reduction_solves_keep_the_serial_pattern_on_2_and_4_ranks() {
    serial_pattern_on_ranks(Law::StiffnessReduction);
}

/// XMAT-010 (UD-870): `orthotropic_damage_iterative` in whole solves:
/// the corrected default damages the global maximum point only (the serial
/// pattern); upstream's code damages the local maximum point of every rank,
/// `n` points at `n` ranks (compat switch), whatever their stress.
#[test]
fn orthotropic_damage_iterative_solves_keep_the_serial_pattern_on_2_and_4_ranks() {
    serial_pattern_on_ranks(Law::Orthotropic);
    let corners = corner_elements();
    for n in [2, 4] {
        let out = run_local(n, TIMEOUT, move |c| {
            let o = Opts {
                orthotropic_damage_global_maximum_only: false,
                ..Opts::default()
            };
            let rank = c.rank();
            let mut model = static_model_with(&c, Law::Orthotropic.dat(), o);
            tension(&mut model)?;
            let b = {
                let mesh = model.physics.base.mesh.read().unwrap();
                element_with_nodes(&mesh, &corners[1])
            };
            if let Some(e) = b {
                Law::Orthotropic.set_sc(&mut model, e, SC_B);
            }
            model
                .solve_step(None)
                .map_err(|e| format!("rank {rank}: solve: {e}"))?;
            let count = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
            let (_, damage) = Law::Orthotropic.state(&mut model);
            let damaged = damage.iter().filter(|&&d| d != 0.).count();
            Ok::<_, String>((count, damaged))
        });
        for (r, o) in out.into_iter().enumerate() {
            match o {
                Ok((count, damaged)) => {
                    assert_eq!(count, u32::try_from(n).unwrap(), "UD-870 compat, {n} ranks");
                    assert_eq!(damaged, 1, "UD-870 compat, {n} ranks, rank {r}");
                }
                Err(e) => panic!("UD-870 compat, {n} ranks, rank {r}: {e}"),
            }
        }
    }
}
