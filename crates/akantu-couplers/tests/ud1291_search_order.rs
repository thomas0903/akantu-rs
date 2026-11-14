// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). UD-1291 (D-109): the
// order of the contact search in the static / implicit solid–contact
// couplers (src/model/model_couplers/coupler_solid_contact_tmpl.hh:66-79,
// 133-145: the search in `corrector()` only;
// src/model/common/non_linear_solver/non_linear_solver_newton_raphson.cc:
// 99-135: the residual test of the predictor before any corrector), with
// closed forms on two elastic blocks pressed together by prescribed
// displacements. No upstream test or example covers it.

//! **Set-up.** Two blocks of two unit quadrangles each (width 2, height 1,
//! `E = 1e3`, `ν = 0`: uniaxial states are exact, the x DOFs carry no
//! load), the lower one on `[0, 2] × [−1, 0]` with its bottom fixed, the
//! upper one on `[0, 2] × [G, 1 + G]` with an initial gap `G = 2⁻⁶`; contact
//! `penalty_linear` (`ε_n = 1e3`, `μ = 0`, deformable master) between the
//! lower block's top (master) and the upper block's bottom (slave nodes at
//! `x = 0, 1, 2`, nodal areas ½, 1, ½; one-pass weighting, the surfaces
//! being distinct). Every x DOF is free except one
//! anchor of the upper block. All values (`G`, the penetrations, the
//! moduli) are dyadic or exact, so the closed forms are exact up to the
//! rounding of the nodal-area integration.
//!
//! **Closed forms.** A penetration `g` of every slave with the master
//! fixed gives the contact pressure `p = ε_n g` (forces `p A_i`); with the
//! master free (the lower block of height 1 compressed in series with the
//! penalty, the slaves prescribed) `p = g / (1/ε_n + 1/E) = ε_n g / 2`.
//!
//! **The defect.** A step that prescribes the displacement of the whole
//! upper block (a rigid translation into the lower one) leaves every free
//! DOF in equilibrium at the predictor: upstream's Newton (`residual`
//! criterion) returns at iteration 0 without a corrector, hence without a
//! search, and the step keeps the previous step's contact state. With
//! [`SolidContactCompat::search_in_corrector_only`] the port reproduces
//! this (the sequence below: one residual event, no corrector, the
//! contact set of the previous step); by default the search before the
//! first residual finds the penetration and the contact pressure of the
//! configuration is there.

mod common;

use akantu_contact::{DeformableMasterWeighting, PhysicalSurfaceSelector};
use akantu_core::Real;
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_couplers::{CouplerSolidContact, SolidContactCallback, SolidContactCompat};
use akantu_mesh::SharedMesh;
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_model::{Model, ModelOptions};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolverOptions};
use common::trace::Traced;
use common::*;

/// Young's modulus of both blocks.
const E: Real = 1e3;
/// Normal penalty.
const EPS_N: Real = 1e3;
/// Initial gap between the blocks (dyadic).
const G: Real = 0.015625;
/// Penetration imposed by the rigid translations (dyadic).
const PEN: Real = 0.0009765625;
/// Relative bound of a contact force against its closed form when nothing
/// is solved (the master fixed): the gaps and pressures are exact (dyadic
/// positions on horizontal segments), the nodal areas come from a two-point
/// Gauss integration of the shape functions on unit segments (½ and 1 up
/// to a few ulp); 1e-14 leaves a factor ≈ 20 above 4 ulp.
const EXACT_REL: Real = 1e-14;
/// Relative bound when the master is solved for: the problem is linear
/// once the contact set is fixed (horizontal faces, slaves prescribed:
/// projections and nodal areas do not change), so one Newton iteration
/// solves it up to the rounding of the sparse LU solve (condition number
/// of the 6 × 6 free system ≲ 1e2) and of the residual test (threshold
/// 1e-10 relative to the step's initial residual ≈ 1): 1e-10.
const SOLVED_REL: Real = 1e-10;

const DAT: &str = "material elastic [\n name = lower\n rho = 1.0\n E = 1e3\n nu = 0.0\n]\n\
material elastic [\n name = upper\n rho = 1.0\n E = 1e3\n nu = 0.0\n]\n\
contact_detector [\n type = explicit\n master = contact_bottom\n slave = contact_top\n \
projection_tolerance = 1e-10\n max_iterations = 100\n extension_tolerance = 1e-5\n]\n\
contact_resolution penalty_linear [\n name = contact_top\n mu = 0.0\n epsilon_n = 1e3\n \
epsilon_t = 1e3\n is_master_deformable = true\n]\n";

/// The two blocks (Gmsh 2.2): nodes 1–6 the lower block (bottom row, then
/// the master row `y = 0`), 7–12 the upper block (slave row `y = G`, then
/// its top).
fn mesh_text() -> String {
    let top = 1.0 + G;
    let mut nodes = String::new();
    let rows = [-1.0, 0.0, G, top];
    let mut id = 1;
    for y in rows {
        for x in [0.0, 1.0, 2.0] {
            nodes.push_str(&format!("{id} {x:?} {y:?} 0\n"));
            id += 1;
        }
    }
    format!(
        "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$PhysicalNames\n6\n1 1 \"contact_bottom\"\n1 2 \
         \"contact_top\"\n1 3 \"lower_bottom\"\n1 4 \"upper_top\"\n2 5 \"lower\"\n2 6 \
         \"upper\"\n$EndPhysicalNames\n$Nodes\n12\n{nodes}$EndNodes\n$Elements\n12\n1 1 2 1 1 4 \
         5\n2 1 2 1 1 5 6\n3 1 2 2 2 7 8\n4 1 2 2 2 8 9\n5 1 2 3 3 1 2\n6 1 2 3 3 2 3\n7 1 2 4 4 \
         10 11\n8 1 2 4 4 11 12\n9 3 2 5 5 1 2 5 4\n10 3 2 5 5 2 3 6 5\n11 3 2 6 6 7 8 11 10\n12 \
         3 2 6 6 8 9 12 11\n$EndElements\n"
    )
}

/// The mesh file, written once per test process (the tests of this binary
/// run in parallel threads: a rewrite could be read half-written).
fn write_mesh() -> std::path::PathBuf {
    static PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("akantu-ud1291-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("two_blocks.msh");
        std::fs::write(&p, mesh_text()).unwrap();
        p
    })
    .clone()
}

/// The node rows (0-based node indices of the mesh).
const LOWER_BOTTOM: [usize; 3] = [0, 1, 2];
const MASTERS: [usize; 3] = [3, 4, 5];
const SLAVES: [usize; 3] = [6, 7, 8];
const UPPER_TOP: [usize; 3] = [9, 10, 11];
/// Nodal areas of the slaves (closed form).
const AREAS: [Real; 3] = [0.5, 1.0, 0.5];

fn coupler(compat: SolidContactCompat) -> CouplerSolidContact {
    coupler_with(compat, NonLinearSolverType::Auto)
}

fn coupler_with(compat: SolidContactCompat, nls_type: NonLinearSolverType) -> CouplerSolidContact {
    let mesh = read_mesh(&write_mesh(), 2);
    let parser = Parser::parse_str(DAT, "two_blocks.dat").unwrap();
    let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
    let mut c = CouplerSolidContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    c.solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    c.init_full(
        &ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
            non_linear_solver_type: Some(nls_type),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        }),
    )
    .unwrap();
    c.contact.set_surface_selector(Box::new(selector)).unwrap();
    // one pass (the master and slave surfaces are distinct): the full
    // force on both sides, not the two-pass weight ½
    c.contact.deformable_master_weighting = DeformableMasterWeighting::OnePassFull;
    c.set_upstream_compat(compat).unwrap();
    if nls_type != NonLinearSolverType::Linear {
        let nls = c.solid.solver.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(100))
            .unwrap();
        nls.set_param("threshold", ParamValue::Real(1e-10)).unwrap();
        nls.set_param("convergence_type", ParamValue::Enum("residual".into()))
            .unwrap();
    }
    c
}

fn compat(search_in_corrector_only: bool) -> SolidContactCompat {
    SolidContactCompat {
        search_in_corrector_only,
        ..SolidContactCompat::default()
    }
}

/// What a load step prescribes in y.
#[derive(Clone, Copy, Debug)]
enum Load {
    /// The upper block's top at `y += top` (relative to its mesh position),
    /// its slave row free; the master row free.
    Top(Real),
    /// Every node of the upper block at `y = v` (relative to its mesh
    /// position: a rigid placement); `masters_blocked`: the master row fixed
    /// at its mesh position.
    Rigid { v: Real, masters_blocked: bool },
    /// Every node of the upper block prescribed at its current y plus `dy`
    /// (a rigid translation that keeps the block's strain); the master row
    /// free.
    Translate(Real),
}

/// The boundary conditions of `load`: the lower block's bottom fixed, the
/// anchor `x = 0` of the upper block's top, every other x DOF free.
fn set_bcs(c: &mut CouplerSolidContact, load: Load) {
    let mut u = c.solid.displacement().unwrap().as_slice().to_vec();
    let mut b = vec![false; u.len()];
    for n in LOWER_BOTTOM {
        u[2 * n] = 0.0;
        u[2 * n + 1] = 0.0;
        b[2 * n] = true;
        b[2 * n + 1] = true;
    }
    u[2 * UPPER_TOP[0]] = 0.0;
    b[2 * UPPER_TOP[0]] = true;
    match load {
        Load::Top(top) => {
            for n in UPPER_TOP {
                u[2 * n + 1] = top;
                b[2 * n + 1] = true;
            }
        }
        Load::Rigid { v, masters_blocked } => {
            for n in SLAVES.iter().chain(&UPPER_TOP) {
                u[2 * n + 1] = v;
                b[2 * n + 1] = true;
            }
            if masters_blocked {
                for n in MASTERS {
                    u[2 * n + 1] = 0.0;
                    b[2 * n + 1] = true;
                }
            }
        }
        Load::Translate(dy) => {
            for n in SLAVES.iter().chain(&UPPER_TOP) {
                u[2 * n + 1] += dy;
                b[2 * n + 1] = true;
            }
        }
    }
    c.solid
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u);
    c.solid
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&b);
}

fn nb_iterations(c: &CouplerSolidContact) -> i64 {
    match c
        .solid
        .solver
        .non_linear_solver(None)
        .unwrap()
        .get_param("nb_iterations")
        .unwrap()
    {
        ParamValue::Int(n) => n,
        other => panic!("{other:?}"),
    }
}

/// The state of the interface after a solve.
#[derive(Debug)]
struct Interface {
    /// Geometric penetration of each slave (from the positions: the master
    /// row's y interpolated at the slave's x, minus the slave's y; > 0 is a
    /// penetration).
    penetration: [Real; 3],
    /// y component of the contact force on each slave.
    slave_force: [Real; 3],
    /// Sum of the y components of the contact force on the master row.
    master_force: Real,
    /// Contact elements of the last search.
    nb_contact: usize,
    /// Newton iterations of the step.
    iterations: i64,
}

fn interface(c: &CouplerSolidContact) -> Interface {
    let x0 = c.solid.mesh().read().unwrap().nodes().as_slice().to_vec();
    let u = c.solid.displacement().unwrap().as_slice().to_vec();
    let y = |n: usize| x0[2 * n + 1] + u[2 * n + 1];
    let x = |n: usize| x0[2 * n] + u[2 * n];
    let f = c.contact.internal_force().as_slice().to_vec();
    let mut penetration = [0.0; 3];
    let mut slave_force = [0.0; 3];
    for (k, &s) in SLAVES.iter().enumerate() {
        // the master facet under the slave (the master row stays straight:
        // its x DOFs carry no load)
        let xs = x(s);
        let (a, b) = if xs <= x(MASTERS[1]) {
            (MASTERS[0], MASTERS[1])
        } else {
            (MASTERS[1], MASTERS[2])
        };
        let t = (xs - x(a)) / (x(b) - x(a));
        let ym = (1.0 - t) * y(a) + t * y(b);
        penetration[k] = ym - y(s);
        slave_force[k] = f[2 * s + 1];
    }
    let master_force = MASTERS.iter().map(|&m| f[2 * m + 1]).sum();
    Interface {
        penetration,
        slave_force,
        master_force,
        nb_contact: c.contact.contact_elements().len(),
        iterations: nb_iterations(c),
    }
}

fn close(a: Real, b: Real, rel: Real) -> bool {
    (a - b).abs() <= rel * a.abs().max(b.abs())
}

/// The contact conditions of the penalty method at a converged step: a
/// penetrating slave carries the pressure of its penetration (`ε_n g A`,
/// pushing it out of the master: +y), a separated one carries nothing, and
/// the master row receives the opposite total. Returns the first violation.
fn contact_conditions(i: &Interface, rel: Real) -> Option<String> {
    for (k, &area) in AREAS.iter().enumerate() {
        let g = i.penetration[k];
        let want = if g > 0.0 { EPS_N * g * area } else { 0.0 };
        let got = i.slave_force[k];
        let ok = if want == 0.0 {
            got == 0.0
        } else {
            close(got, want, rel)
        };
        if !ok {
            return Some(format!(
                "slave {k}: penetration {g:e}, contact force {got:e}, penalty force {want:e}"
            ));
        }
    }
    let total: Real = i.slave_force.iter().sum();
    if !close(i.master_force, -total, rel) && !(total == 0.0 && i.master_force == 0.0) {
        return Some(format!(
            "master force {:e} vs −(slave forces) {:e}",
            i.master_force, -total
        ));
    }
    None
}

/// One step that ends with no contact (the blocks apart), then a rigid
/// translation of the upper block into the lower one by `G + PEN`.
fn rigid_step(search_in_corrector_only: bool, masters_blocked: bool) -> (Interface, Interface) {
    let mut c = coupler(compat(search_in_corrector_only));
    set_bcs(&mut c, Load::Top(0.0));
    c.solve_step(None).unwrap();
    let before = interface(&c);
    set_bcs(
        &mut c,
        Load::Rigid {
            v: -(G + PEN),
            masters_blocked,
        },
    );
    c.solve_step(None).unwrap();
    (before, interface(&c))
}

/// Evidence 1, the master fixed: the step converges at
/// iteration 0 in both modes (every free DOF is an x DOF, unloaded). Under
/// the switch it keeps the previous step's contact state: no contact
/// element and no contact force, with a penetration `PEN` at every slave
/// (upstream's result). By default the search before the first residual
/// finds the penetration: every slave carries `ε_n · PEN · A_i` and the
/// master the opposite.
#[test]
fn ud1291_rigid_translation_converging_at_iteration_0_master_fixed() {
    let (b_up, up) = rigid_step(true, true);
    let (b_de, de) = rigid_step(false, true);
    for b in [&b_up, &b_de] {
        assert_eq!(b.nb_contact, 0, "the blocks start apart: {b:?}");
        assert!(b.penetration.iter().all(|&g| g == -G), "{b:?}");
    }
    eprintln!("switch on: {up:?}\ndefault:   {de:?}");
    for i in [&up, &de] {
        assert_eq!(
            i.iterations, 0,
            "converged at the first residual test: {i:?}"
        );
        assert_eq!(i.penetration, [PEN; 3], "the prescribed penetration: {i:?}");
    }
    // upstream's order: the stale contact state of the separated step
    assert_eq!(up.nb_contact, 0, "{up:?}");
    assert_eq!(up.slave_force, [0.0; 3], "{up:?}");
    assert_eq!(up.master_force, 0.0, "{up:?}");
    assert!(contact_conditions(&up, EXACT_REL).is_some());
    // corrected: the contact pressure of the configuration, p = ε_n g
    assert_eq!(de.nb_contact, 3, "{de:?}");
    for (k, &area) in AREAS.iter().enumerate() {
        let want = EPS_N * PEN * area;
        assert!(
            close(de.slave_force[k], want, EXACT_REL),
            "slave {k}: {} vs ε_n g A = {want}",
            de.slave_force[k]
        );
    }
    assert_eq!(contact_conditions(&de, EXACT_REL), None);
}

/// Evidence 1, the master free: under the switch the step still converges
/// at iteration 0 (the predictor leaves every free DOF in equilibrium) with
/// the penetration `PEN` and no contact force; by default the search before
/// the first residual loads the master row, Newton iterates, and the
/// interface carries the closed-form pressure of the two-spring series
/// `p = PEN / (1/ε_n + 1/E)` (the lower block, height 1, compressed by `w`,
/// the penalty by `PEN − w`), with the masters displaced by `−w = −p/E`.
#[test]
fn ud1291_rigid_translation_master_free_closed_form() {
    let (_, up) = rigid_step(true, false);
    let (_, de) = rigid_step(false, false);
    eprintln!("switch on: {up:?}\ndefault:   {de:?}");
    assert_eq!(up.iterations, 0, "{up:?}");
    assert_eq!(up.nb_contact, 0, "{up:?}");
    assert_eq!(up.slave_force, [0.0; 3], "{up:?}");
    assert_eq!(up.penetration, [PEN; 3], "{up:?}");
    assert!(de.iterations >= 1, "{de:?}");
    let p = PEN / (1.0 / EPS_N + 1.0 / E);
    let w = p / E;
    for (k, &area) in AREAS.iter().enumerate() {
        assert!(
            close(de.slave_force[k], p * area, SOLVED_REL),
            "slave {k}: {} vs p A = {}",
            de.slave_force[k],
            p * area
        );
        assert!(
            close(de.penetration[k], PEN - w, SOLVED_REL),
            "slave {k}: penetration {} vs PEN − w = {}",
            de.penetration[k],
            PEN - w
        );
    }
    assert_eq!(contact_conditions(&de, SOLVED_REL), None);
}

/// The sequence of the step with the switch (upstream's order) and by
/// default, through the coupler's callback: the events of the rigid step
/// (master free) and the number of contact elements seen by the first
/// residual assembly. Upstream: one residual on the previous (empty)
/// contact set, converged, no corrector. Default: the first residual sees
/// the three contact elements of the configuration, then Newton runs
/// (`K`, corrector, residual).
#[test]
fn ud1291_sequence_of_the_step() {
    for search_in_corrector_only in [true, false] {
        let mut c = coupler(compat(search_in_corrector_only));
        set_bcs(&mut c, Load::Top(0.0));
        c.solve_step(None).unwrap();
        set_bcs(
            &mut c,
            Load::Rigid {
                v: -(G + PEN),
                masters_blocked: false,
            },
        );
        let cc = c.upstream_compat();
        let contact = &mut c.contact;
        let mut events: Vec<(String, usize)> = Vec::new();
        c.solid
            .solve_step_with(None, |physics, solver, id| {
                let mut inner =
                    SolidContactCallback::coupler(physics, contact, AnalysisMethod::Static)
                        .with_compat(cc);
                let mut cb = Traced {
                    inner: &mut inner,
                    on_event: |what: &str, _: &DofManager, cbk: &SolidContactCallback<'_>| {
                        events.push((what.to_owned(), cbk.contact.contact_elements().len()));
                    },
                };
                solver.solve_step(&mut cb, id)
            })
            .unwrap();
        eprintln!("search_in_corrector_only = {search_in_corrector_only}: {events:?}");
        if search_in_corrector_only {
            assert_eq!(events, vec![("residual".to_owned(), 0)]);
        } else {
            assert_eq!(events[0], ("residual".to_owned(), 3));
            assert!(
                events.iter().any(|e| e.0 == "corrector"),
                "Newton iterates: {events:?}"
            );
            assert!(events.iter().all(|e| e.1 == 3), "{events:?}");
        }
    }
}

/// Evidence 2: a load path of ten static steps. Steps 0–5:
/// the upper block's top moves down by `0.3 G` per step, its slave row free
/// (contact from step 3 on); every first residual is unbalanced (the top
/// moves alone), so both orders run Newton, and the search before the
/// first residual finds the previous step's last search (the predictor
/// moves no contact node): **the two runs are bitwise equal**. Steps 6–7:
/// the whole upper block is translated down by `PEN` per step (every node
/// prescribed, its strain kept; the master row free); steps 8–9: lifted by
/// `8 PEN` per step (the blocks separate at step 9). In steps 6–9 the
/// predictor leaves every free DOF in equilibrium under the stale contact
/// state: with the switch every one of them converges at iteration 0 and
/// keeps the step-5 contact forces bit for bit (too small for the
/// penetration at 6–7, a force across an open gap at 8–9); the runs differ
/// **from step 6**, the first step whose first residual converges under
/// the switch. The default run satisfies the penalty contact conditions
/// (pressure of the penetration, nothing when separated, the master loaded
/// by the opposite force) at every step.
#[test]
fn ud1291_load_path_default_and_upstream_order() {
    let mut up = coupler(compat(true));
    let mut de = coupler(compat(false));
    let mut first_difference = None;
    let mut its = Vec::new();
    let mut last_force = 0.0;
    let mut forces_of_step_5 = [0.0; 3];
    for step in 0..10 {
        let load = match step {
            0..=5 => Load::Top(-0.3 * G * Real::from(step + 1)),
            6 | 7 => Load::Translate(-PEN),
            _ => Load::Translate(8.0 * PEN),
        };
        set_bcs(&mut up, load);
        set_bcs(&mut de, load);
        up.solve_step(None)
            .unwrap_or_else(|e| panic!("switch on, step {step}: {e}"));
        de.solve_step(None)
            .unwrap_or_else(|e| panic!("default, step {step}: {e}"));
        let iu = interface(&up);
        let id = interface(&de);
        its.push((step, iu.iterations, id.iterations));
        eprintln!("step {step}: switch on {iu:?}\n         default   {id:?}");
        // the corrected run satisfies the contact conditions at every step
        if let Some(v) = contact_conditions(&id, SOLVED_REL) {
            panic!("default, step {step}: {v}");
        }
        let same = up.displacement().unwrap().as_slice() == de.displacement().unwrap().as_slice()
            && up.contact.internal_force().as_slice() == de.contact.internal_force().as_slice();
        if !same && first_difference.is_none() {
            first_difference = Some(step);
        }
        let force: Real = id.slave_force.iter().sum();
        if step <= 5 {
            assert!(same, "step {step}: the runs differ before step 6");
            assert!(iu.iterations >= 1 && id.iterations >= 1, "step {step}");
        } else {
            assert_eq!(iu.iterations, 0, "switch on, step {step}: {iu:?}");
            assert!(id.iterations >= 1, "default, step {step}: {id:?}");
            assert!(
                contact_conditions(&iu, SOLVED_REL).is_some(),
                "switch on, step {step}: the stale state satisfies the contact conditions"
            );
            // the stale forces of step 5, bit for bit
            assert_eq!(iu.slave_force, forces_of_step_5, "switch on, step {step}");
        }
        if step == 5 {
            forces_of_step_5 = iu.slave_force;
        }
        match step {
            0..=2 => assert!(id.penetration.iter().all(|&g| g < 0.0) && force == 0.0),
            3..=5 => assert!(force > last_force, "step {step}: {id:?}"),
            6 | 7 => assert!(force > last_force, "step {step}: {id:?}"),
            8 => {
                // the default run is still in contact, with less force;
                // the stale state already pulls across an open gap
                assert!(force > 0.0 && force < last_force, "step 8: {id:?}");
                assert!(iu.penetration.iter().all(|&g| g < 0.0), "step 8: {iu:?}");
            }
            _ => {
                assert!(id.penetration.iter().all(|&g| g < 0.0), "step 9: {id:?}");
                assert_eq!(force, 0.0, "step 9: {id:?}");
                assert!(iu.penetration.iter().all(|&g| g < 0.0), "step 9: {iu:?}");
                assert!(
                    iu.slave_force.iter().all(|&f| f > 0.0),
                    "step 9: a force across the open gap {iu:?}"
                );
            }
        }
        last_force = force;
    }
    eprintln!("Newton iterations (step, switch on, default): {its:?}");
    assert_eq!(first_difference, Some(6));
}

/// The search owed by the predictor also runs when a solver assembles `K`
/// before the first residual: the linear solver (`NonLinearSolverType::
/// Linear`, upstream `NonLinearSolverLinear`: `J`, the split residual
/// `external − K u` of the stored forces, one solve, corrector) on the
/// rigid step with the master free. The full residual is never assembled
/// on that path (no `residual` event), so the assembly of `K`, the first
/// event of the step, is the only place before the solve where the owed
/// search can run: by default it sees the three contact elements of the
/// predicted configuration (`K` holds their contact stiffness), under the
/// switch none (a search run at the full residual only would give none
/// as well). The values of this solve are not checked: the linear solver
/// treats the penalty as springs between displacements (`K u = f`, the
/// stored contact force), which ignores the initial gap in both orders.
#[test]
fn ud1291_linear_solver_assembles_k_after_the_search() {
    for search_in_corrector_only in [false, true] {
        let mut c = coupler_with(
            compat(search_in_corrector_only),
            NonLinearSolverType::Linear,
        );
        set_bcs(&mut c, Load::Top(0.0));
        c.solve_step(None).unwrap();
        set_bcs(
            &mut c,
            Load::Rigid {
                v: -(G + PEN),
                masters_blocked: false,
            },
        );
        let cc = c.upstream_compat();
        let contact = &mut c.contact;
        let mut events: Vec<(String, usize)> = Vec::new();
        c.solid
            .solve_step_with(None, |physics, solver, id| {
                let mut inner =
                    SolidContactCallback::coupler(physics, contact, AnalysisMethod::Static)
                        .with_compat(cc);
                let mut cb = Traced {
                    inner: &mut inner,
                    on_event: |what: &str, _: &DofManager, cbk: &SolidContactCallback<'_>| {
                        events.push((what.to_owned(), cbk.contact.contact_elements().len()));
                    },
                };
                solver.solve_step(&mut cb, id)
            })
            .unwrap();
        eprintln!(
            "linear solver, search_in_corrector_only = {search_in_corrector_only}: {events:?}"
        );
        let want = if search_in_corrector_only { 0 } else { 3 };
        assert_eq!(events[0], ("matrix:K".to_owned(), want), "{events:?}");
        assert!(events.iter().all(|e| e.0 != "residual"), "{events:?}");
        assert!(events.iter().any(|e| e.0 == "corrector"), "{events:?}");
    }
}

/// UD-1293 (the `solution` criterion, the solvers' default convergence
/// type) on the rigid step with the master free. Under the switch the stale
/// first residual is zero, the first increment `du_0 = 0`, the corrector's
/// search finds the three contact elements, and the step is declared
/// converged after one iteration on `‖du_0‖ = 0`: the residual of the new
/// contact set (assembled after the loop for the outputs) is never tested,
/// and the master row carries the contact force `−ε_n · PEN · A` with no
/// elastic force against it. By default the search before the first
/// residual puts the contact into `r_0`, Newton iterates and the step ends
/// in equilibrium at the closed form (masters at `−PEN/2`).
#[test]
fn ud1293_solution_criterion_accepts_the_untested_last_search() {
    for search_in_corrector_only in [true, false] {
        let mut c = coupler(compat(search_in_corrector_only));
        c.solid
            .solver
            .non_linear_solver_mut(None)
            .unwrap()
            .set_param("convergence_type", ParamValue::Enum("solution".into()))
            .unwrap();
        set_bcs(&mut c, Load::Top(0.0));
        c.solve_step(None).unwrap();
        set_bcs(
            &mut c,
            Load::Rigid {
                v: -(G + PEN),
                masters_blocked: false,
            },
        );
        c.solve_step(None).unwrap();
        let i = interface(&c);
        let u = c.solid.displacement().unwrap().as_slice().to_vec();
        let fi = c.solid.internal_force().unwrap().as_slice().to_vec();
        let fc = c.contact.internal_force().as_slice().to_vec();
        // the residual of the free master DOFs (no external force)
        let residual: Vec<Real> = MASTERS
            .iter()
            .map(|&m| fi[2 * m + 1] + fc[2 * m + 1])
            .collect();
        eprintln!(
            "solution criterion, search_in_corrector_only = {search_in_corrector_only}: {i:?}, \
             master residual {residual:?}"
        );
        assert_eq!(i.nb_contact, 3, "{i:?}");
        if search_in_corrector_only {
            assert_eq!(i.iterations, 1, "{i:?}");
            for (k, &m) in MASTERS.iter().enumerate() {
                assert_eq!(u[2 * m + 1], 0.0, "master {m} moved");
                let want = -EPS_N * PEN * AREAS[k];
                assert!(
                    close(residual[k], want, EXACT_REL),
                    "master {m}: residual {} vs the unbalanced contact force {want}",
                    residual[k]
                );
            }
        } else {
            assert!(i.iterations >= 2, "{i:?}");
            for (k, &m) in MASTERS.iter().enumerate() {
                assert!(close(u[2 * m + 1], -PEN / 2.0, SOLVED_REL), "master {m}");
                assert!(
                    residual[k].abs() <= SOLVED_REL * EPS_N * PEN,
                    "master {m}: residual {}",
                    residual[k]
                );
            }
            assert_eq!(contact_conditions(&i, SOLVED_REL), None);
        }
    }
}
