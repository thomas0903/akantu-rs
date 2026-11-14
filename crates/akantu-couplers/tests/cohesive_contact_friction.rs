// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The solid–cohesive–
// contact coupler (CPL-003, src/model/model_couplers/
// coupler_solid_cohesive_contact.cc, coupler_solid_contact_tmpl.hh) with
// Coulomb friction on the faces of a new crack (penalty_linear with mu > 0,
// src/model/contact_mechanics/resolutions/resolution_penalty.cc) on the mesh
// of examples/c++/contact_mechanics_model/hertz/cohesive_contact_explicit_dynamic.cc.
// No upstream test runs the coupler with friction (the example and both
// coupler fixtures use mu = 0): an independent check with closed forms.

//! **Residual crack shear strength through the coupler** (the paper's claim
//! "interaction between contact and cohesive elements"): a block is cracked
//! by the extrinsic insertion, the crack is broken (zero cohesive traction),
//! closed by compression and then sheared, through
//! `CouplerSolidCohesiveContact` in `_static` with the corrected defaults.
//!
//! **Set-up.** The example's mesh (1 × 2, crack facets on `y = 0`) and
//! contact detector, `E = 1e3`, `ν = 0` (uniaxial states are exact),
//! `cohesive_linear` with `σc = 1`, `G_c = 1e-6` (`δc = 2e-6`), contact
//! `penalty_linear` with `μ = 0.3`, `ε_n = 1e3`, `ε_t = 2e5` (a stiff
//! tangential penalty keeps the slip at the Coulomb limit, `s* = μ p_N /
//! ε_t = 5e-7`, below the example's extension tolerance at the crack ends:
//! every slave stays on its facet). The x displacement of **every** node is
//! prescribed: 0 on the lower block, `s` on the upper block (a rigid
//! translation, no shear strain), so the relative tangential displacement
//! of the crack faces is exactly `s`; the bottom is fixed, the top's y
//! displacement is prescribed, the other y displacements are solved
//! (contact, cohesive and elastic forces). Every contact element is a
//! slave node of one crack face projected on a facet of the other (six,
//! both faces being master and slave), nearly flat and horizontal (the
//! tilt `|τ_y|` is asserted ≤ 5e-9).
//!
//! **Closed forms** (Coulomb law of the penalty resolution, upstream's
//! return map): with `p_N = ε_n g` (`g` the penetration of the slave) and
//! the tangential traction `t` of a slave (its global x component: `t` is a
//! coordinate along the master facet's unit tangent) whose face moved by
//! `±s` relative to its master face since the crack closed,
//! * before the shear: `p_N = d / (2/E + 1/ε_n)` (two blocks of height 1
//!   in series with the penalty; two passes of weight ½), `t = 0`;
//! * stick while `ε_t s ≤ μ p_N`: `t = ∓ε_t s` (the trial traction);
//! * slip above: `t = ∓μ p_N`, opposite to the relative motion (exact: the
//!   return map scales the trial traction to `μ p_N`);
//! * shear reversed by `r` after slipping: stick with `t = t_slip ± ε_t r`
//!   while `|t| ≤ μ p_N`, then slip the other way (`t = ±μ p_N`);
//! * the contact force on the upper block (two passes, weight ½, UD-002)
//!   has tangential part `−½ Σ_slaves t A` and normal part `N = ½ Σ p_N A`;
//!   in slip their ratio is `μ` — **the residual shear strength `μ·N`** —
//!   up to the tilt of the faces, and the elastic force at the blocked top
//!   nodes balances the contact force (y equilibrium).
//!
//! The bounds are rule SW5-1 (PROPOSED, `docs/NUMERICAL_VALIDATION.md`).
//! What kills it: a wrong friction coefficient or penalty,
//! a missing or doubled history commit (the stick traction would not
//! accumulate, the slip state would not persist; mutants: the slip value ×
//! 0.999, no commit of the history in `after_solve_step`: both fail), a
//! wrong slip direction, a contact force not passed to the solid (the y
//! equilibrium), a cohesive traction left on the broken crack.

mod common;

use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_contact::CohesiveSurfaceSelector;
use akantu_core::enums::{AnalysisMethod, ContactState, NonLinearSolverType, SparseSolverType};
use akantu_core::{ElementType, GhostType, Real};
use akantu_couplers::{CouplerSolidCohesiveContact, SolidContactCompat};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolverOptions;
use common::*;

const MU: Real = 0.3;
const EPS_N: Real = 1e3;
const EPS_T: Real = 2e5;

/// Largest tilt `|τ_y|` of the crack faces allowed during the shear: every
/// slave stays on its facet (the projections of the crack ends overshoot
/// the facet end by at most `s / ¼ ≤ 6.4e-6` in the natural coordinate,
/// within the example's extension tolerance 1e-5), so the load is carried
/// almost symmetrically and the faces stay flat up to the extrapolation of
/// the ends, which grows with `s` (measured 2.9e-10 at the first step,
/// 1.1e-9 at the fourth).
const MAX_TILT: Real = 5e-9;
/// Absolute bound of a stick (trial) traction against `ε_t s` (rule SW5-1,
/// PROPOSED, `docs/NUMERICAL_VALIDATION.md`): the trial
/// traction accumulates `ε_t ·` the slip increment, the distance along the
/// master facet between the slave's projection and the master material
/// point of the previous projection. Per step that distance differs from
/// the imposed `Δs` by (i) a few ulp(½) ≈ 1.1e-16 (positions of magnitude
/// ≤ 1, 4 ulp assumed) and (ii) the shift of the projection of a slave that
/// penetrates by `g ≤ 4e-4` when its facet's tilt changes, by at most
/// `2 · MAX_TILT`: `g · 1e-8 = 4e-12`. Over at most 15 steps with `ε_t =
/// 2e5`: 15 · (4.4e-16 + 4e-12) · 2e5 ≈ 1.2e-5 < 1.5e-5 (the tractions are
/// ≈ 0.04…0.1, i.e. ≤ 4e-4 relative; measured deviations ≈ 2e-8 at the
/// first step).
const STICK_ABS: Real = 1.5e-5;
/// Relative bound of a slip traction `μ p_N` and of the shear force `μ N`
/// (SW5-1): `t = μ p_N · t^tr / ‖t^tr‖` with a unit tangent, so a few
/// rounding errors of the norm and the products.
const SLIP_REL: Real = 1e-13;
/// Relative bound of the quantities the Newton solve determines (the
/// penetration against its closed form, the y equilibrium): Newton stops
/// at a residual 1e-10 times the step's initial one (at most ≈ 1e-4 for
/// the forces ≈ 0.3 of these steps), and the solid is linear (SW5-1).
const NEWTON_REL: Real = 1e-9;

fn material() -> String {
    format!(
        "model solid_mechanics_model_cohesive [\n cohesive_inserter [\n  cohesive_surfaces = \
         [interface]\n ]\n material elastic [\n  name = body\n  rho = 1.0\n  E = 1e3\n  nu = \
         0.0\n ]\n material cohesive_linear [\n  name = interface\n  beta = 1\n  G_c = 1e-6\n  \
         sigma_c = 1\n  kappa = 1\n  penalty = 0\n  contact_after_breaking = false\n ]\n]\n\
         contact_detector [\n type = explicit\n master = interface\n slave = interface\n \
         projection_tolerance = 1e-10\n max_iterations = 100\n extension_tolerance = 1e-5\n]\n\
         contact_resolution penalty_linear [\n name = interface\n mu = {MU:?}\n epsilon_n = \
         {EPS_N:e}\n epsilon_t = {EPS_T:e}\n is_master_deformable = true\n]\n"
    )
}

fn coupler(compat: SolidContactCompat) -> CouplerSolidCohesiveContact {
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/cohesive-contact.msh"),
        2,
    );
    let parser = Parser::parse_str(&material(), "material-cohesive-friction.dat").unwrap();
    let mut c = CouplerSolidCohesiveContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    let sel = MeshDataMaterialCohesiveSelector {
        default: c.solid.default_cohesive_selector(),
    };
    c.solid.set_material_selector(Box::new(sel));
    c.set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))
        .unwrap();
    let options =
        ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
            non_linear_solver_type: Some(NonLinearSolverType::Auto),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        });
    c.init_full(&options, true).unwrap();
    c.set_upstream_compat(compat);
    let nls = c.solid.smm.solver.non_linear_solver_mut(None).unwrap();
    nls.set_param("max_iterations", ParamValue::Int(100))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(1e-10)).unwrap();
    // on the residual: the contact set of the last corrector is the one in
    // equilibrium
    nls.set_param("convergence_type", ParamValue::Enum("residual".into()))
        .unwrap();
    c
}

/// `true` for the nodes of the upper block (a node of a quadrangle whose
/// barycentre lies above the crack); after the insertion the crack nodes
/// are doubled, each copy belongs to one block.
fn upper_nodes(c: &CouplerSolidCohesiveContact) -> Vec<bool> {
    let mesh = c.solid.smm.physics.base.mesh.read().unwrap();
    let x = mesh.nodes().as_slice().to_vec();
    let mut up = vec![false; mesh.nb_nodes()];
    let conn = mesh
        .connectivity(ElementType::Quadrangle4, GhostType::NotGhost)
        .unwrap();
    for e in conn.as_slice().as_chunks::<4>().0 {
        let yb: Real = e.iter().map(|&n| x[2 * n as usize + 1]).sum::<Real>() / 4.0;
        if yb > 0.0 {
            for &n in e {
                up[n as usize] = true;
            }
        }
    }
    up
}

/// Every x prescribed (`s` on the upper block, 0 below), the bottom fixed,
/// the top's y at `top`, the other y free.
fn set_bcs(c: &mut CouplerSolidCohesiveContact, s: Real, top: Real) {
    let up = upper_nodes(c);
    let y0 = c
        .solid
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let mut u = c.solid.smm.displacement().unwrap().as_slice().to_vec();
    let mut b = c.solid.smm.blocked_dofs().unwrap().as_slice().to_vec();
    for (n, &is_up) in up.iter().enumerate() {
        u[2 * n] = if is_up { s } else { 0.0 };
        b[2 * n] = true;
        let y = y0[2 * n + 1];
        if y == -1.0 {
            u[2 * n + 1] = 0.0;
            b[2 * n + 1] = true;
        } else if y == 1.0 {
            u[2 * n + 1] = top;
            b[2 * n + 1] = true;
        } else {
            b[2 * n + 1] = false;
        }
    }
    c.solid
        .smm
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u);
    c.solid
        .smm
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&b);
}

fn solve(c: &mut CouplerSolidCohesiveContact, s: Real, top: Real, what: &str) {
    set_bcs(c, s, top);
    c.solve_step(None)
        .unwrap_or_else(|e| panic!("{what} (s = {s:e}, top = {top:e}): {e}"));
}

/// The contact state of the crack after a solve.
struct Crack {
    /// Per contact element: slave, upper (its face), state, the global x
    /// component `t τ_x` of its tangential traction (`t` is a coordinate
    /// along its master facet's unit tangent `τ`, whose orientation
    /// differs between the two faces), `p_N`, area.
    slaves: Vec<(usize, bool, ContactState, Real, Real, Real)>,
    /// Contact force on the upper block: (x, y).
    on_upper: (Real, Real),
    /// Solid internal force at the blocked top nodes, y (the top's
    /// reaction against the block).
    top_reaction_y: Real,
    /// Largest `|τ_y|` of the slaves' tangents (the tilt of the faces).
    max_tilt: Real,
}

fn crack(c: &CouplerSolidCohesiveContact) -> Crack {
    let up = upper_nodes(c);
    let gaps = c.contact.gaps().as_slice().to_vec();
    let area = c.contact.nodal_area().as_slice().to_vec();
    let t = c.contact.tangential_tractions().as_slice().to_vec();
    let tangents = c
        .contact
        .nodal_field_real("tangents")
        .unwrap()
        .as_slice()
        .to_vec();
    let state = c.contact.contact_state().as_slice().to_vec();
    let mut slaves = Vec::new();
    let mut max_tilt: Real = 0.0;
    for el in c.contact.contact_elements() {
        let s = el.slave as usize;
        // nearly horizontal unit tangents (the tilt is checked by the caller)
        let (tx, ty) = (tangents[2 * s], tangents[2 * s + 1]);
        assert!(
            ty.abs() <= 1e-4 && ((tx * tx + ty * ty).sqrt() - 1.0).abs() <= 1e-14,
            "slave {s}: τ = ({tx}, {ty})"
        );
        max_tilt = max_tilt.max(ty.abs());
        slaves.push((
            s,
            up[s],
            state[s],
            t[s] * tx,
            EPS_N * gaps[s].max(0.0),
            area[s],
        ));
    }
    let f = c.contact.internal_force().as_slice().to_vec();
    let mut on_upper = (0.0, 0.0);
    for (n, &is_up) in up.iter().enumerate() {
        if is_up {
            on_upper.0 += f[2 * n];
            on_upper.1 += f[2 * n + 1];
        }
    }
    let y0 = c
        .solid
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let fi = c.solid.smm.internal_force().unwrap().as_slice().to_vec();
    let top_reaction_y = (0..up.len())
        .filter(|&n| y0[2 * n + 1] == 1.0)
        .map(|n| fi[2 * n + 1])
        .sum();
    Crack {
        slaves,
        on_upper,
        top_reaction_y,
        max_tilt,
    }
}

fn close(a: Real, b: Real, rel: Real) -> bool {
    (a - b).abs() <= rel * a.abs().max(b.abs())
}

/// The relative deviation that [`close`] bounds (reported,
/// the measured deviations of SW5-1's Newton-solved quantities).
fn rel_dev(a: Real, b: Real) -> Real {
    (a - b).abs() / a.abs().max(b.abs())
}

fn nb_iterations(c: &CouplerSolidCohesiveContact) -> i64 {
    match c
        .solid
        .smm
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

/// The final error of the last Newton solve (the residual norm under the
/// `residual` criterion).
fn newton_error(c: &CouplerSolidCohesiveContact) -> Real {
    match c
        .solid
        .smm
        .solver
        .non_linear_solver(None)
        .unwrap()
        .get_param("error")
        .unwrap()
    {
        ParamValue::Real(e) => e,
        other => panic!("{other:?}"),
    }
}

/// The compression 1e-3 of the closed crack.
const D: Real = 1e-3;

/// Steps 1–3 of the test: tension (insertion), breaking, compression of
/// the broken crack by `D`, with the closed-form checks of the closed
/// crack. Returns the coupler, the slaves' `p_N` and its closed form.
fn closed_crack(compat: SolidContactCompat) -> (CouplerSolidCohesiveContact, Real, Real) {
    let mut c = coupler(compat);
    // 1. tension: σ = E a / 2 = 1.5 > σc on the interface facets → insertion
    let a = 3e-3;
    solve(&mut c, 0.0, a, "tension");
    let inserted = c.check_cohesive_stress().unwrap();
    assert_eq!(inserted, 2, "both interface facets are inserted");
    // 2. at insertion the cohesive traction equals the insertion stress, an
    // equilibrium; one more tension step breaks the crack (δc = 2e-6 ≪ the
    // opening, no softening state is in equilibrium): zero traction
    solve(&mut c, 0.0, 2.0 * a, "breaking");
    {
        let m = c.solid.cohesive_material("interface").unwrap();
        let d = m
            .array("damage", ElementType::Cohesive2d4, GhostType::NotGhost)
            .unwrap();
        assert!(d.as_slice().iter().all(|&d| d == 1.0), "{:?}", d.as_slice());
    }
    // 3. compression: the crack closes and penetrates
    let d = D;
    for top in [a, 0.0, -d / 2.0, -d] {
        solve(&mut c, 0.0, top, "compression");
    }
    let k0 = crack(&c);
    assert_eq!(
        k0.slaves.len(),
        6,
        "every crack node of both faces is in contact"
    );
    let p_n = k0.slaves[0].4;
    // largest measured deviations of the Newton-solved quantities (rule
    // SW5-1, reported): the penetrations against their closed form, the y
    // equilibrium of the upper block
    let (mut dev_penetration, mut dev_equilibrium): (Real, Real) = (0.0, 0.0);
    for &(s, _, st, t, p, _) in &k0.slaves {
        assert!(p > 0.0, "slave {s}: no penetration");
        // uniaxial compression: the same penetration everywhere
        assert!(close(p, p_n, NEWTON_REL), "slave {s}: p_N {p} vs {p_n}");
        dev_penetration = dev_penetration.max(rel_dev(p, p_n));
        assert_eq!(st, ContactState::Stick, "slave {s} before the shear");
        assert_eq!(t, 0.0, "slave {s}: tangential traction before the shear");
    }
    // the normal force on the upper block (½ per pass, both faces' slaves)
    let normal: Real = k0.slaves.iter().map(|x| 0.5 * x.4 * x.5).sum();
    assert!(
        close(k0.on_upper.1, normal, SLIP_REL),
        "{} vs {normal}",
        k0.on_upper.1
    );
    // uniaxial compression of the two blocks (height 1 each, width 1) in
    // series with the contact penalty (two passes of weight ½: ε_n per
    // pair): d = p (2/E + 1/ε_n)
    let p_exact = d / (2.0 / 1e3 + 1.0 / EPS_N);
    assert!(
        close(normal, p_exact, NEWTON_REL),
        "normal force {normal} vs {p_exact}"
    );
    assert!(close(p_n, p_exact, NEWTON_REL), "p_N {p_n} vs {p_exact}");
    dev_penetration = dev_penetration
        .max(rel_dev(normal, p_exact))
        .max(rel_dev(p_n, p_exact));
    // y equilibrium of the upper block: the elastic force at the blocked top
    // nodes balances the contact force on its crack face
    assert!(
        close(k0.top_reaction_y, normal, NEWTON_REL),
        "top internal force {} vs contact {normal}",
        k0.top_reaction_y
    );
    dev_equilibrium = dev_equilibrium.max(rel_dev(k0.top_reaction_y, normal));
    // the cohesive law transmits nothing on the broken, closed crack
    {
        let m = c.solid.cohesive_material("interface").unwrap();
        let tr = m
            .array("tractions", ElementType::Cohesive2d4, GhostType::NotGhost)
            .unwrap();
        assert!(
            tr.as_slice().iter().all(|&t| t == 0.0),
            "{:?}",
            tr.as_slice()
        );
    }
    eprintln!(
        "closed crack: Newton-solved (relative): penetration vs closed form \
         {dev_penetration:e}, y equilibrium {dev_equilibrium:e}"
    );
    (c, p_n, p_exact)
}

/// Steps 4–5 of the test on the closed crack: the shear and its reversal,
/// with `delta` of additional compression per step; returns the Newton
/// iterations and the final Newton error of every step.
fn shear_and_reversal(
    c: &mut CouplerSolidCohesiveContact,
    p_n: Real,
    p_exact: Real,
    delta: Real,
) -> Vec<(i64, Real)> {
    let d = D;
    let mut iterations = Vec::new();
    // the y equilibrium of the shear steps (rule SW5-1, reported)
    let mut dev_equilibrium: Real = 0.0;
    // 4. shear: stick below the Coulomb limit s* = μ p_N / ε_t, slip above.
    // The upper block is translated rigidly by `s`, so every slave in
    // contact slides by exactly `s` relative to its master face. At the
    // first step the two crack ends whose slave passes the end of its
    // master facet leave the contact (the example's extension tolerance);
    // the four other slaves stay in contact with a fixed contact set. The
    // y displacements then re-balance the load over them and the faces
    // tilt slightly (`max |τ_y|`, measured), which enters only the global
    // force decomposition below.
    // A step whose prescribed values move blocked DOFs only (the shear)
    // leaves the free y DOFs in equilibrium under the previous contact
    // state: in upstream's order (search in the corrector only, UD-1291)
    // the residual test stops Newton before any corrector and the step
    // keeps the contact state of the previous step, so the test was
    // written with `δ = 1e-7` of compression per step (1e-4 of `d`), which
    // unbalances the y DOFs. The corrected default searches before the
    // first residual: the closed forms hold with `δ = 0` as well
    // (`..._without_compression_increments`). The closed forms below use
    // each slave's current `p_N`.
    let s_star = MU * p_n / EPS_T;
    let ds = s_star / 2.5;
    let mut s = 0.0;
    let mut slipping_steps = 0;
    // largest measured deviations (reported)
    let (mut dev_stick, mut dev_slip): (Real, Real) = (0.0, 0.0);
    let mut last_forward: Vec<(usize, Real)> = Vec::new();
    for k in 1..=8 {
        s = ds * Real::from(k);
        solve(c, s, -d - delta * Real::from(k), "shear");
        iterations.push((nb_iterations(c), newton_error(c)));
        let kk = crack(c);
        assert_eq!(kk.slaves.len(), 6, "k {k}: every slave stays in contact");
        assert!(
            kk.max_tilt <= MAX_TILT,
            "k {k}: the faces tilt by {:e}",
            kk.max_tilt
        );
        for &(sl, is_up, st, tx, p, _) in &kk.slaves {
            // the friction on a slave opposes its motion relative to the
            // master face: −x on the upper face, +x on the lower one
            let sign = if is_up { -1.0 } else { 1.0 };
            if EPS_T * s < MU * p {
                assert_eq!(st, ContactState::Stick, "k {k}, slave {sl}");
                assert!(
                    (tx - sign * EPS_T * s).abs() <= STICK_ABS,
                    "k {k}, slave {sl}: stick t {tx} vs {}",
                    sign * EPS_T * s
                );
                dev_stick = dev_stick.max((tx - sign * EPS_T * s).abs());
            } else {
                assert_eq!(st, ContactState::Slip, "k {k}, slave {sl}");
                assert!(
                    close(tx, sign * MU * p, SLIP_REL),
                    "k {k}, slave {sl}: slip t {tx} vs μ p_N = {}",
                    sign * MU * p
                );
                dev_slip = dev_slip.max((tx / (sign * MU * p) - 1.0).abs());
            }
        }
        // equilibrium in y with the current contact set
        assert!(
            close(kk.top_reaction_y, kk.on_upper.1, NEWTON_REL),
            "k {k}: top internal force {} vs the contact force on the block {}",
            kk.top_reaction_y,
            kk.on_upper.1
        );
        dev_equilibrium = dev_equilibrium.max(rel_dev(kk.top_reaction_y, kk.on_upper.1));
        let normal: Real = kk.slaves.iter().map(|x| 0.5 * x.4 * x.5).sum();
        let shear = kk.on_upper.0;
        if kk.slaves.iter().all(|x| x.2 == ContactState::Slip) {
            // the residual shear strength μ·N, up to the tilt of the faces
            let bound = 2.0 * kk.max_tilt * normal + SLIP_REL * MU * normal;
            assert!(
                (shear + MU * normal).abs() <= bound,
                "k {k}: shear force {shear} vs −μ N = {} (bound {bound:e})",
                -MU * normal
            );
            slipping_steps += 1;
        } else {
            assert!(
                shear.abs() < MU * normal,
                "k {k}: stick shear {shear} ≥ μ N"
            );
        }
        last_forward = kk.slaves.iter().map(|x| (x.0, x.3)).collect();
    }
    assert_eq!(slipping_steps, 6, "steps 3 to 8 slip");
    // 5. reversal by r: stick back while the trial traction stays within
    // the Coulomb limit (ε_t r < 2 μ p_N), then slip the other way; `s − r`
    // stays positive, so the contact set is unchanged
    let mut reverse_slip = 0;
    for k in 1..=7 {
        let r = ds * Real::from(k);
        solve(c, s - r, -d - delta * Real::from(8 + k), "reversal");
        iterations.push((nb_iterations(c), newton_error(c)));
        let kk = crack(c);
        assert_eq!(
            kk.slaves.len(),
            6,
            "reversal k {k}: every slave stays in contact"
        );
        assert!(
            kk.max_tilt <= MAX_TILT,
            "reversal k {k}: tilt {:e}",
            kk.max_tilt
        );
        for &(sl, is_up, st, tx, p, _) in &kk.slaves {
            let sign = if is_up { -1.0 } else { 1.0 };
            let t0 = last_forward
                .iter()
                .find(|x| x.0 == sl)
                .map(|x| x.1)
                .unwrap_or_else(|| panic!("reversal k {k}: slave {sl} was not in contact"));
            let trial = t0 - sign * EPS_T * r;
            if trial.abs() <= MU * p {
                assert_eq!(st, ContactState::Stick, "reversal k {k}, slave {sl}");
                assert!(
                    (tx - trial).abs() <= STICK_ABS,
                    "reversal k {k}, slave {sl}: {tx} vs {trial}"
                );
                dev_stick = dev_stick.max((tx - trial).abs());
            } else {
                assert_eq!(st, ContactState::Slip, "reversal k {k}, slave {sl}");
                assert!(
                    close(tx, -sign * MU * p, SLIP_REL),
                    "reversal k {k}, slave {sl}: {tx} vs {}",
                    -sign * MU * p
                );
                dev_slip = dev_slip.max((tx / (-sign * MU * p) - 1.0).abs());
            }
        }
        let normal: Real = kk.slaves.iter().map(|x| 0.5 * x.4 * x.5).sum();
        if kk.slaves.iter().all(|x| x.2 == ContactState::Slip) {
            let bound = 2.0 * kk.max_tilt * normal + SLIP_REL * MU * normal;
            assert!(
                (kk.on_upper.0 - MU * normal).abs() <= bound,
                "reversal k {k}: {} vs μ N = {}",
                kk.on_upper.0,
                MU * normal
            );
            reverse_slip += 1;
        }
    }
    assert_eq!(reverse_slip, 2, "reversal steps 6 and 7 slip the other way");
    let kk = crack(c);
    eprintln!(
        "p_N = {p_n:e} (closed form {p_exact:e}), s* = {s_star:e}; largest deviations: stick \
         |t − closed form| = {dev_stick:e}, slip |t/(μ p_N) − 1| = {dev_slip:e}; Newton-solved \
         (relative): y equilibrium of the shear steps {dev_equilibrium:e}; at the end: max tilt \
         |τ_y| = {:e}, slave p_N {:?}",
        kk.max_tilt,
        kk.slaves.iter().map(|x| x.4).collect::<Vec<_>>()
    );
    iterations
}

/// The test with `δ = 1e-7` of compression per shear step (unbalanced
/// free DOFs at every step: both search orders run Newton).
#[test]
fn residual_crack_shear_strength_through_the_coupler() {
    let (mut c, p_n, p_exact) = closed_crack(SolidContactCompat::default());
    let its = shear_and_reversal(&mut c, p_n, p_exact, 1e-7);
    eprintln!("Newton iterations of the shear and reversal steps: {its:?}");
}

/// Newton threshold of the shear and reversal steps with `δ = 0`. With
/// the corrected order every such step has a first residual ≈ 1e-7 (the
/// slaves at the crack ends pass the end of their master facet by `s`, so
/// their force is extrapolated onto the master's nodes differently): above
/// the absolute threshold 1e-10 of the first test, so Newton iterates, but
/// the relative threshold `1e-10 · r₀ ≈ 1e-17` of the later tests is below
/// the rounding floor of the residual (≈ 1e-16 for forces ≈ 0.3; measured:
/// the first shear step stagnates at 6.5e-17 for 100 iterations, UD-1294).
/// With 1e-8 every step runs one iteration and stops at 1.0e-16…2.5e-16
/// (measured), a tighter equilibrium than the `δ = 1e-7` run (1e-10 times
/// a first residual ≈ 1e-4); the closed-form bounds are unchanged.
fn set_shear_threshold(c: &mut CouplerSolidCohesiveContact) {
    c.solid
        .smm
        .solver
        .non_linear_solver_mut(None)
        .unwrap()
        .set_param("threshold", ParamValue::Real(1e-8))
        .unwrap();
}

/// UD-1291, corrected default (the closed-form test without a 1e-7
/// compression workaround): the shear and reversal steps
/// prescribe the x displacements alone (`δ = 0`), and every closed form
/// holds: the search before the first residual gives each step the contact
/// state of its own configuration (Newton threshold: see
/// [`set_shear_threshold`]).
#[test]
fn residual_crack_shear_strength_without_compression_increments() {
    let (mut c, p_n, p_exact) = closed_crack(SolidContactCompat::default());
    set_shear_threshold(&mut c);
    let its = shear_and_reversal(&mut c, p_n, p_exact, 0.0);
    eprintln!("Newton iterations and errors of the shear and reversal steps (δ = 0): {its:?}");
    // what makes the threshold 1e-8 harmless: no step
    // stops at the absolute iteration-0 test (every first residual ≈ 8.8e-8
    // is above 1e-8), and every step ends at least as close to equilibrium
    // as the δ = 1e-7 test is held to (1e-10 × its first residuals ≈ 1.2e-4)
    for (k, &(n, err)) in its.iter().enumerate() {
        assert!(
            n >= 1,
            "step {k}: converged at the first residual test ({err:e})"
        );
        assert!(err <= 1e-14, "step {k}: final residual {err:e}");
    }
}

/// UD-1291 through `CouplerSolidCohesiveContact::solve_step` with the
/// switch (upstream's order): the first shear step with `δ = 0` converges
/// at iteration 0 (the free y DOFs are in equilibrium under the previous
/// contact state) and keeps the closed crack's contact state: no slave
/// carries a tangential traction although every face slid by `s` (the
/// stick closed form is `ε_t s`). By default the same step has the stick
/// tractions `∓ε_t s`.
#[test]
fn ud1291_switch_keeps_the_crack_state_of_the_previous_step() {
    let mut out = Vec::new();
    for search_in_corrector_only in [true, false] {
        let compat = SolidContactCompat {
            search_in_corrector_only,
            ..SolidContactCompat::default()
        };
        let (mut c, p_n, _) = closed_crack(compat);
        set_shear_threshold(&mut c);
        let s = MU * p_n / EPS_T / 2.5;
        solve(&mut c, s, -D, "shear");
        let kk = crack(&c);
        let its = nb_iterations(&c);
        eprintln!(
            "search_in_corrector_only = {search_in_corrector_only}: {its} iterations, tractions \
             {:?}",
            kk.slaves.iter().map(|x| x.3).collect::<Vec<_>>()
        );
        out.push((its, kk));
        if search_in_corrector_only {
            let (its, kk) = &out[0];
            assert_eq!(*its, 0, "upstream's order: converged at the first residual");
            assert_eq!(kk.slaves.len(), 6);
            for &(sl, _, st, tx, _, _) in &kk.slaves {
                assert_eq!(st, ContactState::Stick, "slave {sl}");
                assert_eq!(
                    tx, 0.0,
                    "slave {sl}: the stale traction of the closed crack"
                );
            }
        } else {
            let (_, kk) = &out[1];
            for &(sl, is_up, st, tx, p, _) in &kk.slaves {
                let sign = if is_up { -1.0 } else { 1.0 };
                assert!(EPS_T * s < MU * p, "slave {sl}: stick expected");
                assert_eq!(st, ContactState::Stick, "slave {sl}");
                assert!(
                    (tx - sign * EPS_T * s).abs() <= STICK_ABS,
                    "slave {sl}: stick t {tx} vs {}",
                    sign * EPS_T * s
                );
            }
        }
    }
}
