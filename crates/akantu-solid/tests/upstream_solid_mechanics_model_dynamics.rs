// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// {test_solid_mechanics_model_dynamics.cc, test_solid_mechanics_model_fixture.hh,
// test_solid_mechanics_model_dynamics_material.dat} and the bar meshes of
// test/test_model/patch_tests/data (registered in the gtest suite
// test_solid_mechanics_model, PARALLEL).
//! Port of `TestSMMFixtureBarExplicit.Dynamics` and
//! `TestSMMFixtureBarImplicit.Dynamics` (TEST-109) with upstream's own
//! analytical reference: the bar meshes (frozen Gmsh meshes of the upstream
//! `add_mesh` targets, D-009), the unit material
//! (`test_solid_mechanics_model_dynamics_material.dat`: E = 1, ρ = 1,
//! **ν = 0**), `Δt = getStableTimeStep()/10`, `max_steps = ⌊5/Δt⌋`; the
//! initial displacement and velocity are upstream's `Verification<dim>` at
//! t = 0, the `BC` group gets the `SolutionFunctor` (x blocked, displacement
//! and velocity set to the analytical wave) before every step; the error
//! `√(Σ_local-or-master |u_ref − u|²) / nb_global_nodes` is maximised over
//! the steps and must satisfy `EXPECT_NEAR(max_error, 0, 2e-3)`.
//!
//! Upstream's `Verification` uses `poisson = 3/10` for the 2-D/3-D wave
//! speeds `c_p = √((λ + 2μ)/ρ)`, `c_s = √(μ/ρ)` while the material file
//! sets ν = 0 (UD-734, category C: the reference wave is not a solution of
//! the simulated material in 2-D/3-D; the weak, `nb_nodes`-normalised error
//! still passes). It is reproduced here (faithful criterion);
//! `model_ported_tests.rs::test_solid_mechanics_model_dynamics_*` checks the
//! same runs against the consistent ν = 0 wave. The 2-D/3-D velocities are
//! upstream's (`cos`, not the time derivative `sin`: part of UD-734).
//!
//! Variants: every element type of `TestElementTypes` (12) explicit; implicit
//! 1-D/2-D in the fast tier; implicit 3-D only in optimised builds (the
//! unoptimised sparse LU needs minutes per bar, as in
//! `model_ported_tests.rs`); the `PARALLEL` registration
//! for the explicit runs at 2 and 4 ranks of the local world (RCB
//! partition); the implicit `PARALLEL` runs at 2 and 4 ranks of the local
//! world, with upstream's skip of `_segment_2`
//! above 2 ranks, against upstream's criterion and against the serial run
//! (gathered final `u`, `v` within `1e-12` of their maximum: the explicit
//! vs-serial value of `par_smm`, a regression guard with margin ≥ 22 on the
//! measured maxima; the a-priori `1e-10` bound of
//! `smm_parallel.rs` `implicit_dynamic_distributed_vs_serial` rests on the
//! cube's conditioning, not the bars'); 1-D/2-D in
//! every build, 3-D in optimised builds only (as the serial 3-D bars). The
//! same implicit runs on real MPI: `mpi_smm.rs` scenario
//! `smm_bar_dynamics_implicit_vs_serial` (par tier, 1/2/4 ranks).

mod smm_bar_dynamics;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Real};
use smm_bar_dynamics::{compare_with_serial, max_error, run_bar};

fn check(ty: ElementType, method: AnalysisMethod) {
    let e = max_error(Communicator::serial(), ty, method);
    eprintln!("{ty} {method:?}: max error {e:e}");
    assert!(e.abs() <= 2e-3, "{ty}: max error {e}");
}

macro_rules! dynamics_tests {
    ($($name:ident: $ty:expr, $method:expr;)*) => {
        $(
            #[test]
            fn $name() {
                check($ty, $method);
            }
        )*
    };
}

use AnalysisMethod::{ExplicitLumpedMass as Ex, ImplicitDynamic as Im};
use ElementType as T;

dynamics_tests! {
    explicit_segment_2: T::Segment2, Ex;
    explicit_segment_3: T::Segment3, Ex;
    explicit_triangle_3: T::Triangle3, Ex;
    explicit_triangle_6: T::Triangle6, Ex;
    explicit_quadrangle_4: T::Quadrangle4, Ex;
    explicit_quadrangle_8: T::Quadrangle8, Ex;
    explicit_tetrahedron_4: T::Tetrahedron4, Ex;
    explicit_tetrahedron_10: T::Tetrahedron10, Ex;
    explicit_hexahedron_8: T::Hexahedron8, Ex;
    explicit_hexahedron_20: T::Hexahedron20, Ex;
    explicit_pentahedron_6: T::Pentahedron6, Ex;
    explicit_pentahedron_15: T::Pentahedron15, Ex;
    implicit_segment_2: T::Segment2, Im;
    implicit_segment_3: T::Segment3, Im;
    implicit_triangle_3: T::Triangle3, Im;
    implicit_triangle_6: T::Triangle6, Im;
    implicit_quadrangle_4: T::Quadrangle4, Im;
    implicit_quadrangle_8: T::Quadrangle8, Im;
}

/// Implicit 3-D bars (optimised builds only, see the module docs).
#[cfg(not(debug_assertions))]
mod implicit_3d {
    use super::*;
    dynamics_tests! {
        implicit_tetrahedron_4: T::Tetrahedron4, Im;
        implicit_tetrahedron_10: T::Tetrahedron10, Im;
        implicit_hexahedron_8: T::Hexahedron8, Im;
        implicit_hexahedron_20: T::Hexahedron20, Im;
        implicit_pentahedron_6: T::Pentahedron6, Im;
        implicit_pentahedron_15: T::Pentahedron15, Im;
    }
}

/// `TestSMMFixtureBarExplicit.Dynamics` under the `PARALLEL` registration:
/// 2 and 4 ranks of the local world, every element type.
#[test]
fn explicit_parallel_2_4_ranks() {
    let types = [
        T::Segment2,
        T::Segment3,
        T::Triangle3,
        T::Triangle6,
        T::Quadrangle4,
        T::Quadrangle8,
        T::Tetrahedron4,
        T::Tetrahedron10,
        T::Hexahedron8,
        T::Hexahedron20,
        T::Pentahedron6,
        T::Pentahedron15,
    ];
    for n in [2, 4] {
        for ty in types {
            let out = run_local(n, Duration::from_secs(600), |c| {
                Ok::<Real, String>(max_error(c, ty, Ex))
            });
            for (r, o) in out.into_iter().enumerate() {
                let e = o.unwrap_or_else(|e| panic!("{n} ranks, rank {r}, {ty}: {e}"));
                assert!(e.abs() <= 2e-3, "{n} ranks, rank {r}, {ty}: max error {e}");
                if r == 0 {
                    eprintln!("{ty} explicit, {n} ranks: max error {e:e}");
                }
            }
        }
    }
}

/// `TestSMMFixtureBarImplicit.Dynamics` under the `PARALLEL` registration:
/// `types` at 2 and 4 ranks of the local world,
/// upstream's skip of `_segment_2` above 2 ranks ("The error are just to
/// high"); upstream's criterion on every rank, and the gathered final state
/// against the serial run (relative `1e-12` of the maximum;
/// measured ≤ 2.1e-14 for 1-D/2-D, 4.4e-14 for 3-D).
fn implicit_parallel(types: &[ElementType]) {
    for &ty in types {
        let ser = run_bar(Communicator::serial(), ty, Im).unwrap_or_else(|e| panic!("{ty}: {e}"));
        for n in [2, 4] {
            if ty == T::Segment2 && n > 2 {
                eprintln!("{ty} implicit, {n} ranks: skipped (upstream)");
                continue;
            }
            let out = run_local(n, Duration::from_secs(600), |c| run_bar(c, ty, Im));
            for (r, o) in out.into_iter().enumerate() {
                let run = o.unwrap_or_else(|e| panic!("{n} ranks, rank {r}, {ty}: {e}"));
                let e = run.max_error;
                assert!(e.abs() <= 2e-3, "{n} ranks, rank {r}, {ty}: max error {e}");
                if r == 0 {
                    let line = compare_with_serial(&run, &ser, VS_SERIAL)
                        .unwrap_or_else(|m| panic!("{n} ranks, {ty} implicit: {m}"));
                    eprintln!("{ty} implicit, {n} ranks: max error {e:e}; vs serial: {line}");
                }
            }
        }
    }
}

/// Bound of the implicit `PARALLEL` runs against the serial run, relative to
/// the field maximum (the explicit vs-serial value of
/// `par_smm`, used as a regression guard; `docs/NUMERICAL_VALIDATION.md`).
const VS_SERIAL: f64 = 1e-12;

#[test]
fn implicit_parallel_2_4_ranks() {
    implicit_parallel(&[
        T::Segment2,
        T::Segment3,
        T::Triangle3,
        T::Triangle6,
        T::Quadrangle4,
        T::Quadrangle8,
    ]);
}

/// Implicit 3-D bars at 2 and 4 ranks (optimised builds only, like the
/// serial 3-D bars).
#[cfg(not(debug_assertions))]
#[test]
fn implicit_parallel_3d_2_4_ranks() {
    implicit_parallel(&[
        T::Tetrahedron4,
        T::Tetrahedron10,
        T::Hexahedron8,
        T::Hexahedron20,
        T::Pentahedron6,
        T::Pentahedron15,
    ]);
}
