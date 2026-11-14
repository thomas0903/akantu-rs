// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regressions of
// UD-352 (D-064) and UD-800: `SolidMechanicsModel::initSolver`
// (src/model/solid_mechanics/solid_mechanics_model.cc:197-241) and
// `updateCurrentPosition` / `getCurrentPosition` (:411-430); no upstream
// test. The oracle replay of nine call orderings is
// `smm_init_solver_orderings_match_the_oracle` (fixtures/model/smm_init_solver,
// probe oracle/probes/smm_init_solver).

//! The current-position cache across a second solver initialisation.
//!
//! Upstream copies `X` into the cache on every `initSolver` without
//! touching its release counter: after a solve, a reader at the current
//! displacement release and `initNewSolver`, every reader (stable time
//! step, cohesive normals, contact) sees the undeformed mesh until the next
//! displacement update (UD-352, category B). The port copies `X` only into
//! a newly allocated cache; `SmmUpstreamCompat::init_solver_resets_current_position`
//! reproduces upstream. Direct writes to the displacement stay invisible
//! until the next release change in both modes (UD-800, category C).

mod model_common;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_parser::Parser;
use akantu_solid::{SmmUpstreamCompat, SolidMechanicsModel};
use akantu_validation::Fixture;
use model_common::{assert_bits_vec, case, f64s, i64s, read_mesh};

const DAT: &str = "material elastic [\n  name = steel\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";

/// A tetrahedron (node 0 clamped, node 1 on y = z = 0, node 2 on z = 0)
/// pulled by a force in z at node 3, initialised for a static solve;
/// returns the model and the mesh nodes.
fn tet(compat: bool) -> (SolidMechanicsModel, Vec<Real>) {
    let mut m = Mesh::new(3, "tet").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost)
            .push_tuple(&[0, 1, 2, 3]);
    }
    m.compute_bounding_box().unwrap();
    let x = m.nodes().as_slice().to_vec();
    let parser = Parser::parse_str(DAT, "material.dat").unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(SharedMesh::new(m), None, "smm", parser).unwrap();
    model
        .physics
        .upstream_compat
        .init_solver_resets_current_position = compat;
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    {
        let mut blocked = model.blocked_dofs_mut().unwrap();
        let b = blocked.as_mut_slice();
        b[..3].fill(true); // node 0
        b[4] = true; // node 1: y, z
        b[5] = true;
        b[8] = true; // node 2: z
    }
    model.external_force_mut().unwrap().as_mut_slice()[11] = 0.1; // node 3, z
    (model, x)
}

fn solved_tet(compat: bool) -> (SolidMechanicsModel, Vec<Real>) {
    let (mut model, x) = tet(compat);
    model.solve_step(None).unwrap();
    assert!(
        model.displacement().unwrap().as_slice()[11] > 0.0,
        "the static solve moved node 3"
    );
    (model, x)
}

fn plus(x: &[Real], u: &[Real]) -> Vec<Real> {
    x.iter().zip(u).map(|(a, b)| a + b).collect()
}

fn cp(model: &mut SolidMechanicsModel) -> Vec<Real> {
    model.current_position().unwrap().as_slice().to_vec()
}

fn u(model: &mut SolidMechanicsModel) -> Vec<Real> {
    model.displacement().unwrap().as_slice().to_vec()
}

/// Ordering B1 (the defect): solve, reader, `initNewSolver`, reader. The
/// default keeps the refreshed `X + u` and the stable time step on it.
#[test]
fn ud352_init_new_solver_keeps_the_refreshed_position() {
    let (mut model, x) = solved_tet(false);
    let deformed = plus(&x, &u(&mut model));
    assert_eq!(cp(&mut model), deformed, "refreshed at the current release");
    let dt = model.stable_time_step().unwrap();
    let releases = (
        model.displacement_release(),
        model.current_position_release(),
    );
    model
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    assert_eq!(
        cp(&mut model),
        deformed,
        "UD-352: X + u after initNewSolver"
    );
    assert_eq!(model.stable_time_step().unwrap(), dt);
    assert_eq!(
        (
            model.displacement_release(),
            model.current_position_release()
        ),
        releases,
        "init_solver leaves both releases alone"
    );
}

/// Ordering B1 in upstream-compat mode: the reset to `X` sticks (UD-352).
#[test]
fn ud352_compat_reset_to_x_sticks_after_a_reader_at_the_current_release() {
    let (mut model, x) = solved_tet(true);
    let _ = cp(&mut model);
    let dt_deformed = model.stable_time_step().unwrap();
    model
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    assert_eq!(cp(&mut model), x, "upstream keeps current_position = X");
    let dt_x = model.stable_time_step().unwrap();
    assert_ne!(dt_x, dt_deformed, "the stable time step is on X upstream");
}

/// Ordering C (no reader before `initNewSolver`): `X + u` in both modes,
/// as upstream (the release differs, the next reader refreshes).
#[test]
fn current_position_is_x_plus_u_after_a_new_solver_without_reader() {
    for compat in [false, true] {
        let (mut model, x) = solved_tet(compat);
        model
            .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
            .unwrap();
        let deformed = plus(&x, &u(&mut model));
        assert_eq!(cp(&mut model), deformed, "compat {compat}");
    }
}

/// Orderings D and D′ (UD-800, C): a displacement written directly before
/// the first solver step is invisible to the readers, with or without an
/// `initNewSolver` in between, in both modes (as upstream).
#[test]
fn ud800_direct_write_before_any_step_stays_invisible() {
    for compat in [false, true] {
        for new_solver_first in [false, true] {
            let (mut model, x) = tet(compat);
            if new_solver_first {
                model
                    .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
                    .unwrap();
            }
            model.displacement_mut().unwrap().as_mut_slice()[11] = 0.25;
            if !new_solver_first {
                model
                    .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
                    .unwrap();
            }
            assert_eq!(
                cp(&mut model),
                x,
                "compat {compat}, initNewSolver first {new_solver_first}"
            );
        }
    }
}

/// Ordering E1: solve, reader, direct write, `initNewSolver`, reader. The
/// default returns the value of the last refresh (`X + u_old`: the direct
/// write is not seen, UD-800); upstream returns `X` (UD-352).
#[test]
fn ud352_direct_write_then_init_new_solver_keeps_the_last_refresh() {
    for compat in [false, true] {
        let (mut model, x) = solved_tet(compat);
        let refreshed = cp(&mut model);
        assert_eq!(refreshed, plus(&x, &u(&mut model)));
        model.displacement_mut().unwrap().as_mut_slice()[11] += 0.25;
        model
            .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
            .unwrap();
        let want = if compat { x.clone() } else { refreshed };
        assert_eq!(cp(&mut model), want, "compat {compat}");
    }
}

/* ------------------------------------------------------------------------ */
/* Oracle replay: nine call orderings                                        */
/* ------------------------------------------------------------------------ */

/// Ordering ids of `fixtures/model/smm_init_solver/orderings` (probe order).
const ORDERINGS: [&str; 9] = ["A", "D", "B0", "B1", "C", "Dp", "E0", "E1", "undeformed"];

/// What one ordering leaves: the stable time step, the current position,
/// the displacement and the releases `(displacement, current position)`,
/// read in the probe's order (`getStableTimeStep()` first).
struct State {
    dt: Real,
    cp: Vec<Real>,
    u: Vec<Real>,
    releases: [i64; 2],
}

fn state(model: &mut SolidMechanicsModel) -> State {
    let dt = model.stable_time_step().unwrap();
    State {
        dt,
        cp: cp(model),
        u: u(model),
        releases: [
            model.displacement_release(),
            model.current_position_release(),
        ],
    }
}

fn static_model(f: &Fixture, compat: bool) -> SolidMechanicsModel {
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model.physics.upstream_compat = SmmUpstreamCompat {
        init_solver_resets_current_position: compat,
        ..SmmUpstreamCompat::default()
    };
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    model
}

/// The three Dirichlet groups of `examples/c++/solid_mechanics_model/static`.
fn static_bcs(model: &mut SolidMechanicsModel) {
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
}

fn new_explicit_solver(model: &mut SolidMechanicsModel) {
    model
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
}

/// The nine orderings of the probe, in its order.
fn run_orderings(f: &Fixture, compat: bool) -> Vec<State> {
    let mut out = Vec::new();
    {
        let mut m = static_model(f, compat);
        static_bcs(&mut m);
        out.push(state(&mut m)); // A
        new_explicit_solver(&mut m);
        out.push(state(&mut m)); // D
    }
    {
        let mut m = static_model(f, compat);
        static_bcs(&mut m);
        m.solve_step(None).unwrap();
        out.push(state(&mut m)); // B0
        new_explicit_solver(&mut m);
        out.push(state(&mut m)); // B1
    }
    {
        let mut m = static_model(f, compat);
        static_bcs(&mut m);
        m.solve_step(None).unwrap();
        new_explicit_solver(&mut m);
        out.push(state(&mut m)); // C
    }
    {
        let mut m = static_model(f, compat);
        new_explicit_solver(&mut m);
        static_bcs(&mut m);
        out.push(state(&mut m)); // Dp
    }
    {
        let mut m = static_model(f, compat);
        static_bcs(&mut m);
        m.solve_step(None).unwrap();
        out.push(state(&mut m)); // E0
        m.apply_dirichlet_bc(
            &IncrementValue {
                value: 1e-4,
                axis: SpatialDirection::X,
            },
            "Displacement",
        )
        .unwrap();
        new_explicit_solver(&mut m);
        out.push(state(&mut m)); // E1
    }
    {
        let mut m = static_model(f, compat);
        out.push(state(&mut m)); // undeformed
    }
    out
}

/// Bitwise comparison of one ordering with the oracle.
fn assert_ordering_bits(f: &Fixture, mode: &str, id: &str, s: &State) {
    let what = |q: &str| format!("{mode} {id}/{q}");
    assert_bits_vec(
        f.case_id(),
        &what("dt"),
        &[s.dt],
        &f64s(f, &format!("{id}/dt")),
    );
    assert_bits_vec(
        f.case_id(),
        &what("current_position"),
        &s.cp,
        &f64s(f, &format!("{id}/current_position")),
    );
    assert_bits_vec(
        f.case_id(),
        &what("displacement"),
        &s.u,
        &f64s(f, &format!("{id}/displacement")),
    );
    assert_eq!(
        s.releases.to_vec(),
        i64s(f, &format!("{id}/releases")),
        "{}",
        what("releases")
    );
}

/// Nine call orderings (solve / reader / `initNewSolver` /
/// direct displacement writes / stable time step) against the pinned
/// x86_64 oracle (`fixtures/model/smm_init_solver/orderings`, D-035):
///
/// * compat mode (`init_solver_resets_current_position`): every quantity
///   of every ordering bitwise, upstream's defect included;
/// * default: bitwise in A, D, B0, C, D′, E0 and the undeformed mesh; in
///   B1 and E1 (the orderings the defect produces) the oracle's
///   displacement and releases bitwise, and the current position and Δt
///   checked independently: B1 holds `X + u` (sums of the oracle's own `X`
///   and `u`) with the Δt upstream computes on `X + u` (B0), E1 the value
///   of the last refresh, `X + u_old` = the oracle's E0 position (the
///   direct write stays invisible, UD-800) with E0's Δt.
#[test]
fn smm_init_solver_orderings_match_the_oracle() {
    let f = case("smm_init_solver/orderings");
    let x = f64s(&f, "mesh/nodes");
    let idx = |id: &str| ORDERINGS.iter().position(|o| *o == id).unwrap();

    let compat = run_orderings(&f, true);
    for (id, s) in ORDERINGS.iter().zip(&compat) {
        assert_ordering_bits(&f, "compat", id, s);
    }

    let default = run_orderings(&f, false);
    for (id, s) in ORDERINGS.iter().zip(&default) {
        if !matches!(*id, "B1" | "E1") {
            assert_ordering_bits(&f, "default", id, s);
        }
    }
    // the defect is on the oracle: X in B1 and E1, Δt on the undeformed mesh
    assert_eq!(f64s(&f, "B1/current_position"), x);
    assert_eq!(f64s(&f, "E1/current_position"), x);
    assert_eq!(f64s(&f, "B1/dt"), f64s(&f, "undeformed/dt"));
    assert_eq!(f64s(&f, "E1/dt"), f64s(&f, "undeformed/dt"));
    assert_ne!(f64s(&f, "B1/dt"), f64s(&f, "B0/dt"));
    // B1: X + u on the oracle's X and u, Δt as upstream's on X + u (B0)
    let b1 = &default[idx("B1")];
    let oracle_u_b1 = f64s(&f, "B1/displacement");
    assert_bits_vec(f.case_id(), "default B1/displacement", &b1.u, &oracle_u_b1);
    assert_eq!(b1.releases.to_vec(), i64s(&f, "B1/releases"));
    assert_bits_vec(
        f.case_id(),
        "default B1/current_position = X + u",
        &b1.cp,
        &plus(&x, &oracle_u_b1),
    );
    assert_bits_vec(f.case_id(), "default B1/dt", &[b1.dt], &f64s(&f, "B0/dt"));
    // E1: the last refresh (E0's X + u), Δt as upstream's in E0
    let e1 = &default[idx("E1")];
    assert_bits_vec(
        f.case_id(),
        "default E1/displacement",
        &e1.u,
        &f64s(&f, "E1/displacement"),
    );
    assert_eq!(e1.releases.to_vec(), i64s(&f, "E1/releases"));
    let oracle_u_e0 = f64s(&f, "E0/displacement");
    assert_bits_vec(
        f.case_id(),
        "default E1/current_position = X + u_old",
        &e1.cp,
        &plus(&x, &oracle_u_e0),
    );
    assert_bits_vec(f.case_id(), "default E1/dt", &[e1.dt], &f64s(&f, "E0/dt"));
    assert_ne!(e1.u, oracle_u_e0, "the direct write changed u");
}
