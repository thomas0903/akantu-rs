// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent checks
// of the traction-at-split-node contact port (D-007): hand-computed
// contact quantities on prescribed nodal fields, closed forms of every
// friction law and regularisation, momentum conservation of the node-to-node
// contact, no penetration and energy decay of a rigid-flat impact, the
// upstream defects UD-039/392…399, restart round trips, parameter access,
// factory coverage (INT-007), boundary-condition helpers.

mod common;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::math::DEFAULT_TOLERANCE;
use akantu_core::{Error, Idx, Real, SliceRef};
use akantu_mesh::Mesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FlagOnly, FromSameDim};
use akantu_ntn_contact::boundary_conditions::{ForceBasedDirichlet, SpringBc};
use akantu_ntn_contact::boundary_functions::{boundary_fix, integrate_residual};
use akantu_ntn_contact::{
    FrictionLaw, ModelState, NtnBaseContact, NtnContactSolverCallback, NtnFriction, Regularisation,
    TasnUpstreamCompat, allocate_friction, initialize_ntn_friction, registered_friction_laws,
};
use akantu_parser::Parser;
use akantu_parser::param::{ParamValue, ParameterRegistry};
use common::*;

/// The oracle's libm `log` (pure-Rust glibc port), as used by the laws:
/// the closed forms below compare exactly.
fn ln(x: f64) -> f64 {
    akantu_core::random::glibc_libm::log(x)
}

const DT: Real = 1e-3;
const DIM: usize = 2;

/// Owned nodal fields of the 12-node mesh `ntn_test_2d.msh` (slider
/// [0,1]×[0,0.25] on base [0,1]×[−0.25,0], interface pairs at x = 0, 1, 0.5).
struct Fields {
    displacement: Vec<Real>,
    velocity: Vec<Real>,
    acceleration: Vec<Real>,
    increment: Vec<Real>,
    mass: Vec<Real>,
    internal_force: Vec<Real>,
    external_force: Vec<Real>,
}

impl Fields {
    fn new(nb_nodes: usize) -> Self {
        let z = vec![0.0; nb_nodes * DIM];
        Self {
            displacement: z.clone(),
            velocity: z.clone(),
            acceleration: z.clone(),
            increment: z.clone(),
            mass: vec![1.0; nb_nodes * DIM],
            internal_force: z.clone(),
            external_force: z,
        }
    }

    fn state<'a>(&'a self, mesh: &'a Mesh) -> ModelState<'a> {
        ModelState {
            mesh,
            spatial_dimension: DIM,
            time_step: DT,
            f_m2a: 1.0,
            displacement: self.displacement.as_slice().into(),
            velocity: self.velocity.as_slice().into(),
            acceleration: self.acceleration.as_slice().into(),
            increment: self.increment.as_slice().into(),
            mass: self.mass.as_slice().into(),
            internal_force: self.internal_force.as_slice().into(),
            external_force: self.external_force.as_slice().into(),
            blocked_dofs: SliceRef::empty(),
            current_position: None,
        }
    }
}

fn mesh_2d() -> Mesh {
    read_mesh(&data("ntn_test_2d.msh"), 2)
}

fn ntn_contact(mesh: &Mesh, fields: &Fields, compat: TasnUpstreamCompat) -> NtnBaseContact {
    let mut c = NtnBaseContact::new_ntn(mesh, DIM, "contact", compat).unwrap();
    c.add_surface_pair(&fields.state(mesh), "slider_bottom", "base_top", 1)
        .unwrap();
    c
}

/// Slaves move towards the masters with normal speed `v` and tangential
/// speed `w` (masters at rest).
fn approach(contact: &NtnBaseContact, f: &mut Fields, v: Real, w: Real) {
    for &s in contact.slaves().as_slice() {
        f.velocity[s as usize * DIM] = w;
        f.velocity[s as usize * DIM + 1] = -v;
    }
}

fn set_t_star(fr: &mut NtnFriction, t: Real) {
    fr.set_param("t_star", ParamValue::Real(t)).unwrap();
}

#[test]
fn contact_quantities_hand_computed() {
    let mesh = mesh_2d();
    let f = Fields::new(mesh.nb_nodes());
    let c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    assert_eq!(c.nb_contact_nodes(), 3);
    let x = mesh.nodes();
    // the middle interface nodes of the upstream mesh differ by 3e-12 in x
    // (the upstream test checks the pairing at 1e-10)
    for (s, m) in c.node_pairs().unwrap() {
        for d in 0..2 {
            assert!((x.tuple(s as usize)[d] - x.tuple(m as usize)[d]).abs() <= 1e-10);
        }
        assert!(x.tuple(s as usize)[1].abs() < 1e-12);
    }
    // outward normal of the base top surface: +y, exactly
    for n in c.normals().as_slice().chunks(2) {
        assert_eq!(n, &[0.0, 1.0]);
    }
    // ∫N over the adjacent interface segments: L/2 per segment (each side
    // has its own middle node: 0.4999999999987 / 0.5000000000021)
    let lumped = |nodes: &[Idx]| -> Vec<Real> {
        let xs: Vec<Real> = nodes.iter().map(|&n| x.tuple(n as usize)[0]).collect();
        let mid = xs.iter().copied().find(|v| (v - 0.5).abs() < 1e-9).unwrap();
        xs.iter()
            .map(|&v| {
                if v == mid {
                    mid / 2.0 + (1.0 - mid) / 2.0
                } else if v < mid {
                    mid / 2.0
                } else {
                    (1.0 - mid) / 2.0
                }
            })
            .collect()
    };
    let ls = lumped(c.slaves().as_slice());
    let lm = lumped(c.masters().unwrap().as_slice());
    for i in 0..3 {
        let lb = c.lumped_boundary_slaves().at(i);
        let lbm = c.lumped_boundary_masters().unwrap().at(i);
        assert!((lb - ls[i]).abs() <= 1e-15, "{lb} vs {}", ls[i]);
        assert!((lbm - lm[i]).abs() <= 1e-15, "{lbm} vs {}", lm[i]);
        // Z = 2/Δt / (A_m/m_m + A_s/m_s), unit masses
        assert_eq!(c.impedance().at(i), 2.0 / DT / (lbm / 1.0 + lb / 1.0));
    }
    assert_eq!(c.node_index(c.slaves().at(0)), Some(0));
    assert_eq!(c.node_index(c.masters().unwrap().at(2)), Some(2));
    assert_eq!(c.node_index(3), None);
}

#[test]
fn contact_pressure_closed_form_and_momentum_of_assembly() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    let v = 0.7;
    approach(&c, &mut f, v, 0.0);
    c.compute_contact_pressure(&f.state(&mesh)).unwrap();
    for i in 0..3 {
        assert!(c.is_in_contact().at(i));
        // predicted gap g = Δt·(−v); p = Z g / (2Δt) n
        let g = 0.0 + DT * -v + DT * DT * 0.0 - DT * DT * 0.0 / 2.0;
        let p = c.impedance().at(i) * g / (2.0 * DT);
        assert_eq!(c.contact_pressure().tuple(i), &[p * 0.0, p * 1.0]);
    }
    c.assemble_global_contact_pressure(&mesh).unwrap();
    // equal lumped areas on both sides (to the 3e-12 non-conformity of the
    // mesh): the contact forces sum to Σ (A_m − A_s) p ≈ 0
    let g = c.global_contact_pressure().as_slice();
    let (fx, fy) = g
        .chunks(2)
        .fold((0.0, 0.0), |(a, b), t| (a + t[0], b + t[1]));
    let scale: Real = g.iter().map(|v| v.abs()).sum();
    assert_eq!(fx, 0.0);
    assert!(fy.abs() <= 1e-11 * scale, "{fy} vs {scale}");
    // the slave is pushed back (+y), the master pushed down
    let s0 = c.slaves().at(0) as usize;
    let m0 = c.masters().unwrap().at(0) as usize;
    assert!(g[s0 * 2 + 1] > 0.0 && g[m0 * 2 + 1] < 0.0);
    // separating: no contact, zero pressure
    approach(&c, &mut f, -v, 0.0);
    c.compute_contact_pressure(&f.state(&mesh)).unwrap();
    for i in 0..3 {
        assert!(!c.is_in_contact().at(i));
        assert_eq!(c.contact_pressure().tuple(i), &[0.0, 0.0]);
    }
    assert_eq!(c.nb_nodes_in_contact(&mesh).unwrap(), 0);
}

#[test]
fn coulomb_stick_and_slip_regimes() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    let mut fr = allocate_friction(&c, mesh.nb_nodes(), "coulomb", "no_regularisation").unwrap();
    let mu = 0.3;
    fr.set_param("mu", ParamValue::Real(mu)).unwrap();
    let v = 0.5;
    for (w, sticks) in [(0.1, true), (0.4, false)] {
        approach(&c, &mut f, v, w);
        let st = f.state(&mesh);
        c.compute_contact_pressure(&st).unwrap();
        fr.compute_friction_traction(&c, &st).unwrap();
        for i in 0..3 {
            let z = c.impedance().at(i);
            let p = c.contact_pressure().tuple(i)[1].abs();
            let strength = mu * p;
            assert_eq!(fr.frictional_strength().at(i), strength);
            // stick traction Z/2 · w (relative tangential velocity w)
            let stick = 1.0 * z * w / 2.0;
            let t = fr.friction_traction().tuple(i);
            assert_eq!(fr.is_sticking().at(i), sticks, "w = {w}");
            if sticks {
                assert_eq!(t, &[stick, 0.0]);
            } else {
                assert!((t[0] - strength).abs() <= 4.0 * f64::EPSILON * strength);
                assert_eq!(t[1], 0.0);
            }
        }
        assert_eq!(
            fr.nb_sticking_nodes(&c, &mesh).unwrap(),
            if sticks { 3 } else { 0 }
        );
    }
}

/// Rubin–Ampuero: with a constant pressure P the frictional pressure after k
/// updates from 0 is P (1 − (1 + α)^−k), α = Δt/t*.
#[test]
fn rubin_ampuero_relaxation_closed_form() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    let mut fr = allocate_friction(&c, mesh.nb_nodes(), "coulomb", "rubin_ampuero").unwrap();
    fr.set_param("mu", ParamValue::Real(0.5)).unwrap();
    let t_star = 4.0 * DT;
    set_t_star(&mut fr, t_star);
    approach(&c, &mut f, 0.3, 0.0);
    let alpha: Real = DT / t_star;
    for k in 1..=20 {
        let st = f.state(&mesh);
        c.compute_contact_pressure(&st).unwrap();
        fr.compute_friction_traction(&c, &st).unwrap();
        for i in 0..3 {
            let p = c.contact_pressure().tuple(i)[1].abs();
            let closed = p * (1.0 - (1.0 + alpha).powi(-k));
            let got = fr.frictional_contact_pressure().at(i);
            assert!(
                (got - closed).abs() <= 1e-14 * p,
                "k = {k}: {got} vs {closed}"
            );
            assert_eq!(fr.frictional_strength().at(i), 0.5 * got);
        }
    }
    // t* = 0 is the unregularised pressure (Math::are_float_equal, tolerance
    // 100 eps)
    set_t_star(&mut fr, 0.5 * DEFAULT_TOLERANCE);
    let st = f.state(&mesh);
    fr.compute_friction_traction(&c, &st).unwrap();
    assert_eq!(
        fr.frictional_contact_pressure().at(0),
        c.contact_pressure().tuple(0)[1].abs()
    );
}

/// Simplified Prakash–Clifton: the strength relaxes towards the Coulomb
/// strength S: s_k = S (1 − (1 + α)^−k); t* = 0 gives S (UD-397) or NaN
/// upstream.
#[test]
fn simplified_prakash_clifton_relaxation_and_ud397() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.0);
    let st = f.state(&mesh);
    c.compute_contact_pressure(&st).unwrap();
    let mut fr =
        allocate_friction(&c, mesh.nb_nodes(), "coulomb", "simplified_prakash_clifton").unwrap();
    fr.set_param("mu", ParamValue::Real(0.5)).unwrap();
    // t* = 0 (default): the unregularised strength
    fr.compute_frictional_strength(&c, &st).unwrap();
    let s_law = 0.5 * c.contact_pressure().tuple(0)[1].abs();
    assert_eq!(fr.frictional_strength().at(0), s_law);
    assert_eq!(fr.spc_internal().unwrap().at(0), s_law);
    // relaxation
    let mut fr =
        allocate_friction(&c, mesh.nb_nodes(), "coulomb", "simplified_prakash_clifton").unwrap();
    fr.set_param("mu", ParamValue::Real(0.5)).unwrap();
    set_t_star(&mut fr, 3.0 * DT);
    let alpha: Real = 1.0 / 3.0;
    for k in 1..=15 {
        fr.compute_frictional_strength(&c, &st).unwrap();
        let closed = s_law * (1.0 - (1.0 + alpha).powi(-k));
        assert!((fr.frictional_strength().at(0) - closed).abs() <= 1e-14 * s_law);
    }
    // steady state: the unregularised strength at once
    fr.set_to_steady_state(&c, &st).unwrap();
    assert_eq!(fr.frictional_strength().at(0), s_law);
    // upstream: alpha = Δt/0 → NaN strength
    let compat = TasnUpstreamCompat {
        spc_zero_t_star_nan: true,
        ..TasnUpstreamCompat::default()
    };
    let c2 = ntn_contact(&mesh, &f, compat);
    let mut fr2 = allocate_friction(
        &c2,
        mesh.nb_nodes(),
        "coulomb",
        "simplified_prakash_clifton",
    )
    .unwrap();
    fr2.compute_frictional_strength(&c2, &st).unwrap();
    assert!(fr2.frictional_strength().at(0).is_nan());
}

/// Drive the slip through `update_slip` with a tangential increment `delta`
/// of the slaves (masters fixed).
fn slip_by(c: &NtnBaseContact, f: &mut Fields, delta: Real) {
    f.increment.fill(0.0);
    for &s in c.slaves().as_slice() {
        f.increment[s as usize * DIM] = delta;
    }
}

#[test]
fn linear_slip_weakening_closed_form_with_and_without_healing() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.0);
    c.compute_contact_pressure(&f.state(&mesh)).unwrap();
    let (mu_s, mu_k, d_c) = (0.6, 0.2, 1e-4);
    for law in ["linear_slip_weakening", "linear_slip_weakening_no_healing"] {
        let mut fr = allocate_friction(&c, mesh.nb_nodes(), law, "no_regularisation").unwrap();
        for (n, v) in [("mu_s", mu_s), ("mu_k", mu_k), ("d_c", d_c)] {
            fr.set_param(n, ParamValue::Real(v)).unwrap();
        }
        // mu is readable only for these laws
        assert!(fr.set_param("mu", ParamValue::Real(1.0)).is_err());
        for i in 0..3 {
            fr.is_sticking_mut().set_at(i, false);
        }
        let delta = 3e-5;
        for step in 1..=5 {
            slip_by(&c, &mut f, delta);
            let st = f.state(&mesh);
            fr.update_slip(&c, &st).unwrap();
            fr.compute_frictional_strength(&c, &st).unwrap();
            let s = fr.slip().at(0);
            let cum = fr.cumulative_slip().at(0);
            assert!((cum - step as Real * delta).abs() < 1e-18);
            let used = if law.ends_with("healing") { cum } else { s };
            let mu = if used >= d_c {
                mu_k
            } else {
                mu_k + (1.0 - (used / d_c)) * (mu_s - mu_k)
            };
            let p = c.contact_pressure().tuple(0)[1].abs();
            assert_eq!(fr.frictional_strength().at(0), mu * p, "{law} step {step}");
            assert_eq!(fr.law_param("mu").unwrap().at(0), mu);
        }
        // sticking: slip reset (healing) / cumulative slip kept
        for i in 0..3 {
            fr.is_sticking_mut().set_at(i, true);
        }
        let st = f.state(&mesh);
        fr.update_slip(&c, &st).unwrap();
        fr.compute_frictional_strength(&c, &st).unwrap();
        assert_eq!(fr.slip().at(0), 0.0);
        let mu = fr.law_param("mu").unwrap().at(0);
        if law.ends_with("healing") {
            assert_eq!(mu, mu_k, "no healing: cumulative slip beyond d_c");
        } else {
            assert_eq!(mu, mu_s, "healing: static coefficient while sticking");
        }
    }
}

#[test]
fn linear_cohesive_closed_form() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.0);
    c.compute_contact_pressure(&f.state(&mesh)).unwrap();
    let mut fr =
        allocate_friction(&c, mesh.nb_nodes(), "linear_cohesive", "no_regularisation").unwrap();
    let (g_c, tau_c, tau_r) = (2.0, 5.0, 1.0);
    for (n, v) in [("G_c", g_c), ("tau_c", tau_c), ("tau_r", tau_r)] {
        fr.set_param(n, ParamValue::Real(v)).unwrap();
    }
    assert!(fr.get_param("mu").is_err(), "no Coulomb coefficient");
    for i in 0..3 {
        fr.is_sticking_mut().set_at(i, false);
    }
    let slope = (tau_c - tau_r) * (tau_c - tau_r) / (2.0 * g_c);
    for _ in 0..6 {
        slip_by(&c, &mut f, 0.2);
        let st = f.state(&mesh);
        fr.update_slip(&c, &st).unwrap();
        fr.compute_frictional_strength(&c, &st).unwrap();
        let s = fr.cumulative_slip().at(0);
        let expected = (tau_c - slope * s).max(tau_r);
        assert_eq!(fr.frictional_strength().at(0), expected);
    }
    // G_c = 0: residual strength; out of contact: zero
    fr.set_param("G_c", ParamValue::Real(0.0)).unwrap();
    let st = f.state(&mesh);
    fr.compute_frictional_strength(&c, &st).unwrap();
    assert_eq!(fr.frictional_strength().at(1), tau_r);
    drop(st);
    approach(&c, &mut f, -0.3, 0.0);
    let st = f.state(&mesh);
    c.compute_contact_pressure(&st).unwrap();
    fr.compute_frictional_strength(&c, &st).unwrap();
    assert_eq!(fr.frictional_strength().at(1), 0.0);
}

/// Every rate-and-state law at steady sliding: θ = d_c/V leaves the ageing
/// state unchanged (to rounding) and the coefficient matches its closed form.
#[test]
fn rate_and_state_steady_state_closed_forms() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.0);
    c.compute_contact_pressure(&f.state(&mesh)).unwrap();
    let (mu_0, a, b, v_star, phi_star, d_c) = (0.28, 0.005, 0.075, 1e-7, 0.00033, 5e-7);
    let v = 2e-3;
    let theta = d_c / v;
    for name in registered_friction_laws()
        .iter()
        .filter(|n| n.starts_with("rate_and_state:"))
    {
        if name.ends_with("flashheating") {
            continue;
        }
        let mut fr = allocate_friction(&c, mesh.nb_nodes(), name, "no_regularisation").unwrap();
        for (n, val) in [
            ("mu_0", mu_0),
            ("a", a),
            ("b", b),
            ("v_star", v_star),
            ("phi_star", phi_star),
            ("d_c", d_c),
            ("v0", 0.0),
            ("friction_state", theta),
        ] {
            fr.set_param(n, ParamValue::Real(val)).unwrap();
        }
        for i in 0..3 {
            fr.is_sticking_mut().set_at(i, false);
        }
        slip_by(&c, &mut f, v * DT);
        let st = f.state(&mesh);
        fr.update_slip(&c, &st).unwrap();
        let vs = fr.slip_velocity_norm().at(0);
        assert!((vs - v).abs() < 1e-15);
        fr.compute_frictional_strength(&c, &st).unwrap();
        let th = fr.friction_state().at(0);
        // ageing / regularised ageing (v0 = 0) / slip laws are stationary
        assert!(
            (th - theta).abs() <= 1e-12 * theta,
            "{name}: θ {th} vs {theta}"
        );
        let r = vs / v_star;
        let closed = match name.split(':').nth(1).unwrap() {
            "dieterich" => mu_0 + a * ln(r) + b * ln(th / phi_star),
            "revised_dieterich" => mu_0 + a * ln(1.0 + r) + b * ln(th / phi_star) * mu_0,
            "regularized_dieterich" => mu_0 + a * ln(1.0 + r) + b * ln(1.0 + th / phi_star) * mu_0,
            "n_shaped" => {
                (1.0 + b * ln(1.0 + th / phi_star)) * (mu_0 / (1.0 + r * r).sqrt() + a * ln(r))
            }
            "revised_n_shaped" => {
                let q = v_star / vs;
                (1.0 + b * ln(1.0 + th / phi_star))
                    * (mu_0 / (1.0 + q * q).sqrt() + a * ln(1.0 + r))
            }
            other => panic!("{other}"),
        };
        let mu = fr.law_param("mu").unwrap().at(0);
        assert_eq!(mu, closed, "{name}");
        let p = c.contact_pressure().tuple(0)[1].abs();
        assert_eq!(fr.frictional_strength().at(0), mu * p);
        // Dieterich steady-state friction: μ_ss = μ0 + (a − b) ln(V/V*) + b ln(d_c/(V* θ*))
        if name == "rate_and_state:dieterich:ageing" {
            let mu_ss = mu_0 + (a - b) * ln(v / v_star) + b * ln(d_c / (v_star * phi_star));
            assert!((mu - mu_ss).abs() < 1e-12);
        }
    }
}

/// The flash-heating law (TASN-034) at
/// steady sliding, which `rate_and_state_steady_state_closed_forms` skips.
/// With the ageing state at its steady value `θ = d_c/V` (stationary to
/// rounding) the coefficient is the flash-heating steady state
/// `μ_ss(V) = μ_w + (μ_0 + (a − b) ln(V/V*) + b ln(d_c/(V* φ*)) − μ_w) /
/// (1 + V/v_w)` (`d_c/(θ v_w) = V/v_w`), at velocities from far below the
/// weakening velocity `v_w` to far above it: `μ_ss` decreases with `V`
/// (`a < b` and the flash weakening) and approaches `μ_w` (within 1 % of
/// `μ_w` at `V = 10⁴ v_w`); below `V*` the state is pinned at `d_c/V*`.
/// The closed form is evaluated in another order than the law (`ln(V/V*)`
/// and `ln(d_c/(V* φ*))` instead of `ln(θ/φ*)`): rule SW5-2 (PROPOSED),
/// `1e-14` absolute on coefficients of order 1 (a few ulp of each term).
#[test]
fn flash_heating_steady_state_closed_form() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.0);
    c.compute_contact_pressure(&f.state(&mesh)).unwrap();
    let (mu_0, a, b, v_star, phi_star, d_c, mu_w, v_w) =
        (0.6, 0.01, 0.014, 1e-6, 0.4, 4e-4, 0.13, 0.1);
    let closed = |v: Real| {
        mu_w + (mu_0 + (a - b) * ln(v / v_star) + b * ln(d_c / (v_star * phi_star)) - mu_w)
            / (1.0 + v / v_w)
    };
    let mut previous = Real::INFINITY;
    for v in [1e-4, 1e-2, 0.1, 1.0, 10.0, 1e3] {
        let theta = d_c / v;
        let mut fr = allocate_friction(
            &c,
            mesh.nb_nodes(),
            "rate_and_state:flashheating",
            "no_regularisation",
        )
        .unwrap();
        for (n, val) in [
            ("mu_0", mu_0),
            ("a", a),
            ("b", b),
            ("v_star", v_star),
            ("phi_star", phi_star),
            ("d_c", d_c),
            ("mu_w", mu_w),
            ("v_w", v_w),
            ("friction_state", theta),
        ] {
            fr.set_param(n, ParamValue::Real(val)).unwrap();
        }
        for i in 0..3 {
            fr.is_sticking_mut().set_at(i, false);
        }
        slip_by(&c, &mut f, v * DT);
        let st = f.state(&mesh);
        fr.update_slip(&c, &st).unwrap();
        let vs = fr.slip_velocity_norm().at(0);
        assert!((vs - v).abs() <= 1e-15 * v, "slip velocity {vs} vs {v}");
        fr.compute_frictional_strength(&c, &st).unwrap();
        let th = fr.friction_state().at(0);
        assert!(
            (th - theta).abs() <= 1e-12 * theta,
            "V = {v}: θ {th} vs {theta}"
        );
        let mu = fr.law_param("mu").unwrap().at(0);
        let want = closed(v);
        assert!(
            (mu - want).abs() <= 1e-14,
            "V = {v}: μ {mu} vs the steady state {want}"
        );
        assert!(mu < previous, "V = {v}: μ_ss {mu} does not decrease");
        previous = mu;
        let p = c.contact_pressure().tuple(0)[1].abs();
        assert_eq!(fr.frictional_strength().at(0), mu * p);
        drop(st);
    }
    // far above v_w the coefficient approaches the weakened value μ_w
    assert!(
        previous > mu_w && previous - mu_w < 0.01 * mu_w,
        "{previous}"
    );
    // below V*: the state is pinned at d_c/V* whatever it was
    let mut fr = allocate_friction(
        &c,
        mesh.nb_nodes(),
        "rate_and_state:flashheating",
        "no_regularisation",
    )
    .unwrap();
    for (n, val) in [
        ("mu_0", mu_0),
        ("a", a),
        ("b", b),
        ("v_star", v_star),
        ("phi_star", phi_star),
        ("d_c", d_c),
        ("mu_w", mu_w),
        ("v_w", v_w),
        ("friction_state", 1.0),
    ] {
        fr.set_param(n, ParamValue::Real(val)).unwrap();
    }
    for i in 0..3 {
        fr.is_sticking_mut().set_at(i, false);
    }
    slip_by(&c, &mut f, 0.25 * v_star * DT);
    let st = f.state(&mesh);
    fr.update_slip(&c, &st).unwrap();
    fr.compute_frictional_strength(&c, &st).unwrap();
    assert_eq!(fr.friction_state().at(0), d_c / v_star);
}

/// UD-039: at rest (sticking, zero slip velocity) the Dieterich coefficient
/// is −∞: domain error by default, non-finite tractions upstream.
#[test]
fn ud039_singular_rate_and_state_at_rest() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    approach_all(&mesh, &mut f);
    for (compat, expect_error) in [
        (TasnUpstreamCompat::default(), true),
        (TasnUpstreamCompat::upstream(), false),
    ] {
        let mut c = ntn_contact(&mesh, &f, compat);
        let mut fr = allocate_friction(
            &c,
            mesh.nb_nodes(),
            "rate_and_state:dieterich:ageing",
            "no_regularisation",
        )
        .unwrap();
        for (n, val) in [
            ("mu_0", 0.28),
            ("a", 0.005),
            ("b", 0.075),
            ("v_star", 1e-7),
            ("phi_star", 3.3e-4),
            ("d_c", 5e-7),
            ("friction_state", 2.5e-4),
        ] {
            fr.set_param(n, ParamValue::Real(val)).unwrap();
        }
        let st = f.state(&mesh);
        c.compute_contact_pressure(&st).unwrap();
        fr.update_slip(&c, &st).unwrap(); // default: sticking → V = 0
        match fr.compute_friction_traction(&c, &st) {
            Err(Error::Numerical(m)) => {
                assert!(expect_error && m.contains("UD-039"), "{m}");
            }
            Ok(()) => {
                assert!(!expect_error);
                assert_eq!(fr.law_param("mu").unwrap().at(0), Real::NEG_INFINITY);
                assert_eq!(fr.frictional_strength().at(0), Real::NEG_INFINITY);
                // the stick traction is scaled by α = −∞
                assert!(
                    fr.friction_traction()
                        .as_slice()
                        .iter()
                        .any(|t| !t.is_finite())
                );
            }
            Err(e) => panic!("{e}"),
        }
    }
    // the laws regular at rest stay finite in the corrected mode
    for name in [
        "rate_and_state:revised_dieterich:ageing",
        "rate_and_state:regularized_dieterich:ageing",
        "rate_and_state:revised_n_shaped:ageing",
        "rate_and_state:flashheating",
    ] {
        let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
        let mut fr = allocate_friction(&c, mesh.nb_nodes(), name, "no_regularisation").unwrap();
        for (n, val) in [
            ("mu_0", 0.28),
            ("a", 0.005),
            ("b", 0.075),
            ("v_star", 1e-7),
            ("phi_star", 3.3e-4),
            ("d_c", 5e-7),
            ("friction_state", 2.5e-4),
        ] {
            fr.set_param(n, ParamValue::Real(val)).unwrap();
        }
        let st = f.state(&mesh);
        c.compute_contact_pressure(&st).unwrap();
        fr.update_slip(&c, &st).unwrap();
        fr.compute_friction_traction(&c, &st).unwrap();
        assert!(
            fr.friction_traction()
                .as_slice()
                .iter()
                .all(|t| t.is_finite()),
            "{name}"
        );
    }
}

fn approach_all(mesh: &Mesh, f: &mut Fields) {
    // slaves of ntn_test_2d.msh: the slider_bottom nodes
    let slaves: Vec<Idx> = mesh
        .groups()
        .element_group("slider_bottom")
        .unwrap()
        .nodes()
        .to_vec();
    for s in slaves {
        f.velocity[s as usize * DIM] = 0.1;
        f.velocity[s as usize * DIM + 1] = -0.4;
    }
}

#[test]
fn flash_heating_branches_through_the_friction() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.0);
    c.compute_contact_pressure(&f.state(&mesh)).unwrap();
    let mut fr = allocate_friction(
        &c,
        mesh.nb_nodes(),
        "rate_and_state:flashheating",
        "no_regularisation",
    )
    .unwrap();
    // V Δt / d_c = 0.1 at the fast slip below: stable explicit state update
    let (mu_0, a, b, v_star, phi_star, d_c, mu_w, v_w) =
        (0.3, 0.01, 0.02, 1e-3, 1e-2, 1e-3, 0.1, 0.2);
    for (n, val) in [
        ("mu_0", mu_0),
        ("a", a),
        ("b", b),
        ("v_star", v_star),
        ("phi_star", phi_star),
        ("d_c", d_c),
        ("mu_w", mu_w),
        ("v_w", v_w),
        ("friction_state", 1e-3),
    ] {
        fr.set_param(n, ParamValue::Real(val)).unwrap();
    }
    // sticking: μ0
    let st = f.state(&mesh);
    fr.update_slip(&c, &st).unwrap();
    fr.compute_frictional_strength(&c, &st).unwrap();
    assert_eq!(fr.law_param("mu").unwrap().at(0), mu_0);
    // slow slip (V < V*): θ = d_c/V*
    for i in 0..3 {
        fr.is_sticking_mut().set_at(i, false);
    }
    drop(st);
    slip_by(&c, &mut f, 0.5 * v_star * DT);
    let st = f.state(&mesh);
    fr.update_slip(&c, &st).unwrap();
    fr.compute_frictional_strength(&c, &st).unwrap();
    assert_eq!(fr.friction_state().at(0), d_c / v_star);
    let mu = fr.law_param("mu").unwrap().at(0);
    assert_eq!(
        mu,
        mu_0 + a * ln(v_star / v_star) + b * ln(d_c / v_star / phi_star)
    );
    // fast slip: weakening towards μ_w
    drop(st);
    slip_by(&c, &mut f, 100.0 * v_star * DT);
    let st = f.state(&mesh);
    fr.update_slip(&c, &st).unwrap();
    let th0 = fr.friction_state().at(0);
    fr.compute_frictional_strength(&c, &st).unwrap();
    let vs = fr.slip_velocity_norm().at(0);
    let th = th0 + (1.0 - th0 * vs / d_c) * DT;
    assert_eq!(fr.friction_state().at(0), th);
    let mu_rs = mu_0 + a * ln(vs / v_star) + b * ln(th / phi_star);
    assert_eq!(
        fr.law_param("mu").unwrap().at(0),
        mu_w + (mu_rs - mu_w) / (1.0 + d_c / (th * v_w))
    );
}

#[test]
fn parameters_access_and_parsing() {
    let mesh = mesh_2d();
    let f = Fields::new(mesh.nb_nodes());
    let c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    let mut fr = allocate_friction(&c, mesh.nb_nodes(), "coulomb", "no_regularisation").unwrap();
    // per-node parameter: get = default, set = every node + default
    fr.set_param("mu", ParamValue::Real(0.4)).unwrap();
    assert_eq!(fr.get_param("mu").unwrap().as_real().unwrap(), 0.4);
    assert_eq!(fr.law_param("mu").unwrap().as_slice(), &[0.4; 3]);
    let node = c.slaves().at(1);
    fr.set_param_at_node(&c, "mu", node, 0.9).unwrap();
    assert_eq!(fr.law_param("mu").unwrap().as_slice(), &[0.4, 0.9, 0.4]);
    // not a contact node: ignored (warning), as upstream
    fr.set_param_at_node(&c, "mu", 3, 7.0).unwrap();
    // internal parameter: neither readable nor writable
    assert!(fr.get_param("frictional_contact_pressure").is_err());
    assert!(
        fr.set_param_at_node(&c, "frictional_contact_pressure", node, 1.0)
            .is_err()
    );
    assert!(
        fr.get_param("t_star").is_err(),
        "no t_star without regularisation"
    );
    // parsing a friction section
    let p = Parser::parse_str("friction coulomb [\n mu = 0.25\n]\n", "f.dat").unwrap();
    let fr = initialize_ntn_friction(&c, mesh.nb_nodes(), &p).unwrap();
    assert_eq!(fr.regularisation(), Regularisation::NoRegularisation);
    assert_eq!(fr.law_param("mu").unwrap().as_slice(), &[0.25; 3]);
    let p = Parser::parse_str(
        "friction coulomb [\n mu = 0.25\n t_star = 1e-3\n]\n",
        "f.dat",
    )
    .unwrap();
    assert!(
        initialize_ntn_friction(&c, mesh.nb_nodes(), &p).is_err(),
        "t_star unknown without regularisation"
    );
    let p = Parser::parse_str(
        "friction coulomb rubin_ampuero [\n mu = 0.25\n t_star = 1e-3\n]\n",
        "f.dat",
    )
    .unwrap();
    let fr = initialize_ntn_friction(&c, mesh.nb_nodes(), &p).unwrap();
    assert_eq!(fr.t_star().unwrap().as_slice(), &[1e-3; 3]);
    // UD-039 compat: the default option allocates Rubin–Ampuero (t_star accepted)
    let cu = ntn_contact(&mesh, &f, TasnUpstreamCompat::upstream());
    let p = Parser::parse_str(
        "friction coulomb [\n mu = 0.25\n t_star = 1e-3\n]\n",
        "f.dat",
    )
    .unwrap();
    let fr = initialize_ntn_friction(&cu, mesh.nb_nodes(), &p).unwrap();
    assert_eq!(fr.regularisation(), Regularisation::RubinAmpuero);
    // the first of several friction sections
    let p = Parser::parse_str(
        "friction coulomb [\n mu = 0.1\n]\nfriction linear_cohesive [\n G_c = 1\n]\n",
        "f.dat",
    )
    .unwrap();
    assert_eq!(
        initialize_ntn_friction(&c, mesh.nb_nodes(), &p)
            .unwrap()
            .law(),
        FrictionLaw::Coulomb
    );
    let p = Parser::parse_str("material elastic [\n E = 1\n]\n", "f.dat").unwrap();
    assert!(
        initialize_ntn_friction(&c, mesh.nb_nodes(), &p).is_err(),
        "no friction section"
    );
}

/// INT-007: every registered law × regularisation for both contact kinds.
#[test]
fn factory_instantiates_every_combination() {
    let mesh = mesh_2d();
    let f = Fields::new(mesh.nb_nodes());
    let ntn = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    let mut ntrf =
        NtnBaseContact::new_ntrf(&mesh, DIM, "contact", TasnUpstreamCompat::default()).unwrap();
    ntrf.set_normal(0.0, 1.0, 0.0).unwrap();
    ntrf.add_surface(&f.state(&mesh), "slider_bottom").unwrap();
    let mut n = 0;
    for contact in [&ntn, &ntrf] {
        for law in registered_friction_laws() {
            for reg in [
                "no_regularisation",
                "rubin_ampuero",
                "simplified_prakash_clifton",
            ] {
                let fr = allocate_friction(contact, mesh.nb_nodes(), &law, reg).unwrap();
                assert_eq!(fr.regularisation().name(), reg);
                assert_eq!(fr.is_node_to_node(), contact.is_ntn_contact());
                assert_eq!(fr.is_sticking().len(), contact.nb_contact_nodes());
                n += 1;
            }
            assert!(allocate_friction(contact, mesh.nb_nodes(), &law, "rubin").is_err());
        }
        assert!(allocate_friction(contact, mesh.nb_nodes(), "nope", "no_regularisation").is_err());
    }
    assert_eq!(n, 2 * 23 * 3);
}

#[test]
fn noise_on_the_state_variable() {
    let mesh = mesh_2d();
    let f = Fields::new(mesh.nb_nodes());
    let c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    let mut fr = allocate_friction(
        &c,
        mesh.nb_nodes(),
        "rate_and_state:dieterich:ageing",
        "no_regularisation",
    )
    .unwrap();
    fr.set_param("friction_state", ParamValue::Real(2.0))
        .unwrap();
    let st = f.state(&mesh);
    // keys: first key ≥ x
    fr.add_noise_to_state_variable(&c, &st, &[(0.25, 0.5), (0.75, -0.25), (1.5, 0.1)])
        .unwrap();
    for (i, &s) in c.slaves().as_slice().iter().enumerate() {
        let x = mesh.nodes().tuple(s as usize)[0];
        let p = if x <= 0.25 {
            0.5
        } else if x <= 0.75 {
            -0.25
        } else {
            0.1
        };
        assert_eq!(fr.friction_state().at(i), 2.0 * (1.0 + p));
    }
    // UD-396: no key at or above x → error; other laws → not implemented
    assert!(
        fr.add_noise_to_state_variable(&c, &st, &[(0.25, 0.5)])
            .is_err()
    );
    let mut co = allocate_friction(&c, mesh.nb_nodes(), "coulomb", "no_regularisation").unwrap();
    assert!(matches!(
        co.add_noise_to_state_variable(&c, &st, &[(2.0, 0.1)]),
        Err(Error::NotImplemented(_))
    ));
}

#[test]
fn restart_round_trips_and_upstream_no_op() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let dir = std::env::temp_dir().join(format!("ntn-restart-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let prefix = dir.join("r").to_string_lossy().into_owned();
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.2);
    let st = f.state(&mesh);
    c.compute_contact_pressure(&st).unwrap();
    let mut fr = allocate_friction(
        &c,
        mesh.nb_nodes(),
        "linear_slip_weakening",
        "rubin_ampuero",
    )
    .unwrap();
    for (n, v) in [
        ("mu_s", 0.5),
        ("mu_k", 0.2),
        ("d_c", 1e-3),
        ("t_star", 2e-3),
    ] {
        fr.set_param(n, ParamValue::Real(v)).unwrap();
    }
    for i in 0..3 {
        fr.is_sticking_mut().set_at(i, false);
    }
    drop(st);
    slip_by(&c, &mut f, 1e-4);
    let st = f.state(&mesh);
    fr.update_slip(&c, &st).unwrap();
    fr.compute_friction_traction(&c, &st).unwrap();
    c.dump_restart(&prefix).unwrap();
    fr.dump_restart(&c, &prefix).unwrap();
    let round = |x: Real| -> Real { format!("{x:.11e}").parse::<Real>().unwrap() };
    let _ = round;
    // corrected: a fresh friction restores everything, incl. the slip (UD-395)
    let mut fresh = allocate_friction(
        &c,
        mesh.nb_nodes(),
        "linear_slip_weakening",
        "rubin_ampuero",
    )
    .unwrap();
    fresh.read_restart(&c, &prefix).unwrap();
    for (a, b) in fresh.slip().as_slice().iter().zip(fr.slip().as_slice()) {
        assert!((a - b).abs() <= 1e-11 * b.abs(), "slip {a} vs {b}");
    }
    for (a, b) in fresh
        .friction_traction()
        .as_slice()
        .iter()
        .zip(fr.friction_traction().as_slice())
    {
        assert!((a - b).abs() <= 1e-11 * b.abs().max(1e-300));
    }
    assert_eq!(fresh.law_param("mu_s").unwrap().as_slice(), &[0.5; 3]);
    assert_eq!(fresh.is_sticking().as_slice(), fr.is_sticking().as_slice());
    // upstream: nothing is read (UD-394), the slip file is skipped (UD-395)
    let cu = ntn_contact(&mesh, &f, TasnUpstreamCompat::upstream());
    let mut up = allocate_friction(
        &cu,
        mesh.nb_nodes(),
        "linear_slip_weakening",
        "rubin_ampuero",
    )
    .unwrap();
    up.read_restart(&cu, &prefix).unwrap();
    assert_eq!(up.slip().as_slice(), &[0.0; 3]);
    assert_eq!(up.law_param("mu_s").unwrap().as_slice(), &[0.0; 3]);
    // the contact itself
    let mut c2 = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    c2.read_restart(&prefix).unwrap();
    assert_eq!(c2.slaves().as_slice(), c.slaves().as_slice());
    for (a, b) in c2
        .contact_pressure()
        .as_slice()
        .iter()
        .zip(c.contact_pressure().as_slice())
    {
        assert!((a - b).abs() <= 1e-11 * b.abs().max(1e-300));
    }
    // a contact with another node count refuses the files
    let mut ntrf =
        NtnBaseContact::new_ntrf(&mesh, DIM, "contact", TasnUpstreamCompat::default()).unwrap();
    ntrf.add_nodes(&f.state(&mesh), &[0]).unwrap();
    assert!(ntrf.read_restart(&prefix).is_err());
    std::fs::remove_dir_all(&dir).ok();
}

/// Nodes added to the contact after the friction exists: the friction arrays
/// follow with their (possibly parsed) default values, as upstream's
/// dependent arrays.
#[test]
fn friction_follows_nodes_added_later() {
    let mesh = mesh_2d();
    let f = Fields::new(mesh.nb_nodes());
    let mut c =
        NtnBaseContact::new_ntrf(&mesh, DIM, "contact", TasnUpstreamCompat::default()).unwrap();
    c.set_normal(0.0, 1.0, 0.0).unwrap();
    c.add_nodes(&f.state(&mesh), &[0]).unwrap();
    let mut fr = allocate_friction(&c, mesh.nb_nodes(), "coulomb", "no_regularisation").unwrap();
    fr.set_param("mu", ParamValue::Real(0.35)).unwrap();
    c.add_nodes(&f.state(&mesh), &[1, 8]).unwrap();
    fr.sync_with(&c).unwrap();
    assert_eq!(fr.law_param("mu").unwrap().as_slice(), &[0.35; 3]);
    assert_eq!(fr.is_sticking().as_slice(), &[true; 3]);
    // a friction of another contact is refused
    let other =
        NtnBaseContact::new_ntrf(&mesh, DIM, "other", TasnUpstreamCompat::default()).unwrap();
    assert!(fr.sync_with(&other).is_err());
}

fn write_mesh(name: &str, text: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("ntn-mesh-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, text).unwrap();
    p
}

/// UD-392: a slave close to two masters (non-conforming interface).
#[test]
fn ud392_pairing_stops_at_the_first_master() {
    // slaves at x = 0, 1, 2; masters at x = 0, 0.05, 1, 2 (y = 0)
    let msh = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$PhysicalNames\n2\n1 1 \"slave\"\n1 2 \"master\"\n$EndPhysicalNames\n$Nodes\n7\n1 0 0 0\n2 1 0 0\n3 2 0 0\n4 0 0 0\n5 0.05 0 0\n6 1 0 0\n7 2 0 0\n$EndNodes\n$Elements\n5\n1 1 2 1 1 1 2\n2 1 2 1 1 2 3\n3 1 2 2 2 4 5\n4 1 2 2 2 5 6\n5 1 2 2 2 6 7\n$EndElements\n";
    let mesh = read_mesh(&write_mesh("ud392.msh", msh), 2);
    let fixed = NtnBaseContact::pair_interface_nodes(&mesh, "slave", "master", 1, false).unwrap();
    let up = NtnBaseContact::pair_interface_nodes(&mesh, "slave", "master", 1, true).unwrap();
    assert_eq!(fixed.len(), 3);
    assert_eq!(
        up.len(),
        4,
        "upstream pairs slave 0 with both masters near x = 0"
    );
    assert!(NtnBaseContact::pair_interface_nodes(&mesh, "slave", "master", 2, false).is_err());
}

/// UD-393: 1-D pairing stores the actual nodes (upstream: node 0 with 0).
#[test]
fn ud393_one_dimensional_pairing() {
    let msh = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$PhysicalNames\n2\n0 1 \"slave\"\n0 2 \"master\"\n$EndPhysicalNames\n$Nodes\n3\n1 0 0 0\n2 1 0 0\n3 1 0 0\n$EndNodes\n$Elements\n2\n1 15 2 1 1 2\n2 15 2 2 2 3\n$EndElements\n";
    let mesh = read_mesh(&write_mesh("ud393.msh", msh), 1);
    let pairs = NtnBaseContact::pair_interface_nodes(&mesh, "slave", "master", 0, false).unwrap();
    assert_eq!(pairs, vec![(1, 2)]);
}

fn friction_dat(friction: &str) -> String {
    format!(
        "{friction}\nmaterial elastic [\n name = slider\n rho = 1200\n nu = 0.33\n E = 7.98e9\n Plane_Stress = 1\n]\nmaterial elastic [\n name = base\n rho = 1200\n nu = 0.33\n E = 7.98e9\n Plane_Stress = 1\n]\n"
    )
}

/// Node-to-node contact and friction forces are equal and opposite up to the
/// lumped-area difference of each pair: with no boundary condition the total
/// momentum is conserved through an oblique impact with sliding.
///
/// Rule NTN-MOM (docs/NUMERICAL_VALIDATION.md): the slave receives
/// `−A_s (p + τ)` and the master
/// `+A_m (p + τ)`, so the momentum change is exactly the signed area term
/// `D = Σ_steps Δt Σ_pairs (A_m − A_s)(p + τ)` plus rounding. On
/// `ntn_test_compression.msh` the paired lumped areas differ by up to
/// 9.6e-12 relative (not zero), so `D` is computed here a posteriori:
/// `|ΔP − D| ≤ 1e-13 |P₀|` (rounding: ≈ n_steps · 4ε ≈ 9e-14 for 200 steps;
/// observed 1.7e-15) and `|ΔP| ≤ |D| + rounding ≤ 1e-12 |P₀|` (observed
/// |D| ≤ 6.3e-14 |P₀|).
#[test]
fn ntn_impact_conserves_momentum() {
    let mesh = read_mesh(&data("ntn_test_compression.msh"), 2);
    let parser =
        Parser::parse_str(&friction_dat("friction coulomb [\n mu = 0.3\n]"), "m.dat").unwrap();
    let mut model = model(mesh, parser);
    let mut cb = NtnContactSolverCallback::new(
        &mut model,
        "slider_bottom",
        "base_top",
        1,
        0.5,
        TasnUpstreamCompat::default(),
    )
    .unwrap();
    let slider: Vec<Idx> = {
        let mh = model.physics.base.mesh.clone();
        let m = mh.read().unwrap();
        m.groups().element_group("slider").unwrap().nodes().to_vec()
    };
    {
        let mut v_g = model.velocity_mut().unwrap();
        let v = v_g.as_mut_slice();
        for &n in &slider {
            v[n as usize * 2] = 0.5;
            v[n as usize * 2 + 1] = -1.0;
        }
    }
    let momentum = |m: &akantu_solid::SolidMechanicsModel| -> [Real; 2] {
        let v_g = m.velocity().unwrap();
        let v = v_g.as_slice();
        let ms_g = m.mass().unwrap();
        let ms = ms_g.as_slice();
        let mut p = [0.0; 2];
        for i in 0..v.len() {
            p[i % 2] += ms[i] * v[i];
        }
        p
    };
    let p0 = momentum(&model);
    let scale = p0[0].abs().max(p0[1].abs());
    let lbs = cb.contact().lumped_boundary_slaves().as_slice().to_vec();
    let lbm = cb
        .contact()
        .lumped_boundary_masters()
        .unwrap()
        .as_slice()
        .to_vec();
    let dt = model.time_step(None).unwrap();
    let mut area_term = [0.0; 2];
    let mut contacts = 0;
    let mut slipping = 0;
    for _ in 0..200 {
        cb.solve_step(&mut model, None).unwrap();
        let p = momentum(&model);
        let cp = cb.contact().contact_pressure().as_slice();
        let ft = cb.friction().friction_traction().as_slice();
        for (i, (s, m)) in lbs.iter().zip(&lbm).enumerate() {
            for (k, d) in area_term.iter_mut().enumerate() {
                *d += dt * (m - s) * (cp[2 * i + k] + ft[2 * i + k]);
            }
        }
        for k in 0..2 {
            let dp = p[k] - p0[k];
            assert!(
                (dp - area_term[k]).abs() <= 1e-13 * scale,
                "component {k}: ΔP {dp:e} vs area term {:e} (|P₀| {scale:e})",
                area_term[k]
            );
            assert!(dp.abs() <= 1e-12 * scale, "{p:?} vs {p0:?}");
        }
        contacts += cb
            .contact()
            .is_in_contact()
            .as_slice()
            .iter()
            .filter(|&&b| b)
            .count();
        slipping += cb
            .friction()
            .is_sticking()
            .as_slice()
            .iter()
            .filter(|&&b| !b)
            .count();
    }
    assert!(
        contacts > 0 && slipping > 0,
        "the impact must produce contact and sliding"
    );
}

/// A block thrown onto a rigid plane (rigid-flat contact, frictionless): no
/// penetration beyond rounding once in contact, and the mechanical energy
/// never grows (the split-node impulse is perfectly plastic).
#[test]
fn ntrf_impact_no_penetration_and_energy_decay() {
    let mesh = read_mesh(&data("ntn_test_compression.msh"), 2);
    let parser =
        Parser::parse_str(&friction_dat("friction coulomb [\n mu = 0.0\n]"), "m.dat").unwrap();
    let mut model = model(mesh, parser);
    let mh = model.physics.base.mesh.clone();
    let mut contact = {
        let m = mh.read().unwrap();
        NtnBaseContact::new_ntrf(&m, 2, "contact", TasnUpstreamCompat::default()).unwrap()
    };
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model.assemble_mass_lumped().unwrap();
    contact.set_reference_point(0.0, -1e-3, 0.0).unwrap();
    contact.set_normal(0.0, 1.0, 0.0).unwrap();
    {
        let m = mh.read().unwrap();
        contact
            .add_surface(
                &ModelState::of_model(&m, &mut model).unwrap(),
                "slider_bottom",
            )
            .unwrap();
    }
    let dt = model.stable_time_step().unwrap() * 0.5;
    model.set_time_step(dt, None).unwrap();
    let friction = {
        let m = mh.read().unwrap();
        let st = ModelState::of_model(&m, &mut model).unwrap();
        contact.update_impedance(&st).unwrap();
        drop(st);
        initialize_ntn_friction(&contact, m.nb_nodes(), &model.physics.base.parser).unwrap()
    };
    let mut cb = NtnContactSolverCallback::from_parts(contact, friction);
    // the base is fixed; the slider falls at 20 m/s
    model
        .apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::X,
            },
            "base",
        )
        .unwrap();
    model
        .apply_dirichlet_bc(
            &FlagOnly {
                axis: SpatialDirection::Y,
            },
            "base",
        )
        .unwrap();
    let slider: Vec<Idx> = mh
        .read()
        .unwrap()
        .groups()
        .element_group("slider")
        .unwrap()
        .nodes()
        .to_vec();
    {
        let mut v_g = model.velocity_mut().unwrap();
        let v = v_g.as_mut_slice();
        for &n in &slider {
            v[n as usize * 2 + 1] = -20.0;
        }
    }
    let e0 = model.energy("kinetic").unwrap() + model.energy("potential").unwrap();
    let mut impacted = false;
    for _ in 0..300 {
        cb.solve_step(&mut model, None).unwrap();
        let e = model.energy("kinetic").unwrap() + model.energy("potential").unwrap();
        assert!(e <= e0 * (1.0 + 1e-12), "energy grew: {e} > {e0}");
        let m = mh.read().unwrap();
        let st = ModelState::of_model(&m, &mut model).unwrap();
        let gaps = cb.contact().compute_normal_gap(&st).unwrap();
        if cb.contact().is_in_contact().as_slice().iter().any(|&b| b) {
            impacted = true;
        }
        if impacted {
            for g in gaps {
                assert!(g >= -1e-12, "penetration {g}");
            }
        }
        // frictionless: no friction force
        assert!(
            cb.friction()
                .friction_traction()
                .as_slice()
                .iter()
                .all(|&t| t == 0.0)
        );
    }
    assert!(impacted);
}

/// MIIASYM = rigid flat with half the impedance and doubled relative fields;
/// its `computeContactPressure` is a no-op.
#[test]
fn miiasym_relations_to_rigid_flat() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    for (i, v) in f.velocity.iter_mut().enumerate() {
        *v = 0.1 * (i as Real) - 0.3;
    }
    let st = f.state(&mesh);
    let mut r = NtnBaseContact::new_ntrf(&mesh, DIM, "c", TasnUpstreamCompat::default()).unwrap();
    let mut m =
        NtnBaseContact::new_miiasym(&mesh, DIM, "c", TasnUpstreamCompat::default()).unwrap();
    for c in [&mut r, &mut m] {
        c.set_reference_point(0.0, 0.01, 0.0).unwrap();
        c.set_normal(0.6, 0.8, 0.0).unwrap();
        c.add_surface(&st, "slider_bottom").unwrap();
    }
    for i in 0..3 {
        assert_eq!(m.impedance().at(i), r.impedance().at(i) * 0.5);
    }
    let rn = r
        .compute_relative_normal_field(&st.velocity, mesh.nb_nodes())
        .unwrap();
    let mn = m
        .compute_relative_normal_field(&st.velocity, mesh.nb_nodes())
        .unwrap();
    let rt = r
        .compute_relative_tangential_field(&st.velocity, mesh.nb_nodes())
        .unwrap();
    let mt = m
        .compute_relative_tangential_field(&st.velocity, mesh.nb_nodes())
        .unwrap();
    for (a, b) in mn.iter().zip(&rn) {
        assert_eq!(*a, 2.0 * b);
    }
    for (a, b) in mt.iter().zip(&rt) {
        assert_eq!(*a, 2.0 * b);
    }
    // the gap is not doubled
    assert_eq!(
        m.compute_normal_gap(&st).unwrap(),
        r.compute_normal_gap(&st).unwrap()
    );
    let before = m.contact_pressure().as_slice().to_vec();
    m.compute_contact_pressure(&st).unwrap();
    assert!(
        m.contact_pressure()
            .as_slice()
            .iter()
            .zip(&before)
            .all(|(a, b)| a.to_bits() == b.to_bits())
    );
    m.compute_contact_pressure_in_equilibrium(&st).unwrap();
    assert!(
        m.contact_pressure()
            .as_slice()
            .iter()
            .all(|p| p.is_finite())
    );
    assert!(r.compute_contact_pressure_in_equilibrium(&st).is_err());
    // unit plane normal
    assert_eq!(r.plane_normal().unwrap(), &[0.6 / 1.0, 0.8 / 1.0]);
}

/// The whole slider moved rigidly by a force-based condition: no internal
/// force, so the increments follow the velocity-Verlet closed form
/// Σ value_k = a Δt² K(K+1)/2 with a = F/m.
#[test]
fn force_based_dirichlet_rigid_translation() {
    let mesh = read_mesh(&data("ntn_test_compression.msh"), 2);
    let parser = Parser::parse_str(&friction_dat(""), "m.dat").unwrap();
    let mut model = model(mesh, parser);
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let dt = 1e-6;
    model.set_time_step(dt, None).unwrap();
    model.assemble_internal_forces().unwrap();
    let (force, mass) = (8.0, 2.0);
    let mut bc = ForceBasedDirichlet::new(SpatialDirection::X, force, mass);
    bc.insert_sub_boundary("slider");
    let a = force / mass;
    let mut total = 0.0;
    for k in 1..=10 {
        let reaction = bc.apply_yourself(&mut model).unwrap();
        model.assemble_internal_forces().unwrap();
        assert!(
            reaction.abs() < 1e-6,
            "rigid translation: no internal force ({reaction})"
        );
        total += bc.value();
        let closed = a * dt * dt * (k * (k + 1)) as Real / 2.0;
        assert!(
            (total - closed).abs() <= 1e-12 * closed,
            "{total} vs {closed}"
        );
        assert!((bc.velocity() - a * dt * k as Real).abs() <= 1e-12 * a * dt * k as Real);
    }
    let mh = model.physics.base.mesh.clone();
    let m = mh.read().unwrap();
    let n0 = m
        .groups()
        .element_group("slider")
        .unwrap()
        .nodes()
        .as_slice()[0] as usize;
    assert!((model.displacement().unwrap().as_slice()[n0 * 2] - total).abs() < 1e-18);
}

#[test]
fn spring_bc_equilibrium_and_integrate_residual() {
    let mesh = read_mesh(&data("ntn_test_compression.msh"), 2);
    let parser = Parser::parse_str(&friction_dat(""), "m.dat").unwrap();
    let mut model = model(mesh, parser);
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    {
        let mut u_g = model.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for (i, v) in u.iter_mut().enumerate() {
            *v = if i % 2 == 1 { -1e-5 * (i as Real) } else { 0.0 };
        }
    }
    model.assemble_internal_forces().unwrap();
    let r = integrate_residual("slider_top", &model, 1).unwrap();
    let mh = model.physics.base.mesh.clone();
    let manual: Real = {
        let m = mh.read().unwrap();
        let fi_g = model.internal_force().unwrap();
        let fi = fi_g.as_slice();
        m.groups()
            .element_group("slider_top")
            .unwrap()
            .nodes()
            .iter()
            .fold(0.0, |s, &n| s + fi[n as usize * 2 + 1])
    };
    assert_eq!(r, manual);
    assert!(integrate_residual("slider_top", &model, 2).is_err());
    let k = 3e8;
    let mut spring = SpringBc::new(SpatialDirection::Y, k, 5.0);
    spring.base_mut().insert_sub_boundary("slider_top");
    spring.set_to_equilibrium(&model).unwrap();
    assert_eq!(spring.base().target_force(), -r);
    assert_eq!(spring.elongation(), -(-r) / k);
    spring.increment_elongation(1e-6);
    assert_eq!(spring.elongation(), r / k + 1e-6);
}

#[test]
fn boundary_fix_creates_missing_groups() {
    let mut mesh = mesh_2d();
    boundary_fix(&mut mesh, &["slider_top", "missing_one"], false).unwrap();
    assert!(mesh.groups().element_group_exists("missing_one"));
    assert_eq!(
        mesh.groups()
            .element_group("missing_one")
            .unwrap()
            .nb_nodes(),
        0
    );
    // UD-398 compat: upstream throws on an existing group
    assert!(boundary_fix(&mut mesh, &["slider_top"], true).is_err());
    boundary_fix(&mut mesh, &["another"], true).unwrap();
    assert!(!mesh.groups().element_group_exists("another"));
}

#[test]
fn manual_restart_round_trip_with_a_model() {
    use akantu_ntn_contact::manual_restart::{dump_restart, load_restart};
    let parser = Parser::parse_str(&friction_dat(""), "m.dat").unwrap();
    let mut a = model(
        read_mesh(&data("ntn_test_compression.msh"), 2),
        parser.clone(),
    );
    a.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    for (i, v) in a
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .iter_mut()
        .enumerate()
    {
        *v = (i as Real).sin() * 1e-3;
    }
    let p = std::env::temp_dir().join(format!("ntn-manual-{}.txt", std::process::id()));
    dump_restart(&a, &p).unwrap();
    let mut b = model(read_mesh(&data("ntn_test_compression.msh"), 2), parser);
    b.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    load_restart(&mut b, &p).unwrap();
    for (x, y) in a
        .displacement()
        .unwrap()
        .as_slice()
        .iter()
        .zip(b.displacement().unwrap().as_slice())
    {
        // %.9e: 10 significant digits
        assert!((x - y).abs() <= 5e-10 * x.abs().max(1e-300));
    }
    std::fs::remove_file(&p).ok();
    let _ = FromSameDim::new(vec![0.0, 0.0]);
}

/// D-105 / UD-1280: the manual restart writes and reads
/// the displacement only (upstream's format: no external force either), so
/// the record of the Neumann loads (`SolidMechanicsModel::neumann_store`) is
/// neither written nor restored: a restart leaves a model's record and
/// external force as they are, and a model restarted from the file has no
/// record until its own Neumann applications rebuild it.
#[test]
fn manual_restart_neither_writes_nor_restores_the_neumann_record() {
    use akantu_ntn_contact::manual_restart::{dump_restart, load_restart};
    let parser = Parser::parse_str(&friction_dat(""), "m.dat").unwrap();
    let mut a = model(
        read_mesh(&data("ntn_test_compression.msh"), 2),
        parser.clone(),
    );
    a.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    a.apply_neumann_bc(&FromSameDim::new(vec![0.0, -1.0e6]), "slider_top")
        .unwrap();
    let record = format!("{:?}", a.neumann_store().unwrap().forces());
    let stats = a.neumann_store().unwrap().stats();
    let f = a.external_force().unwrap().as_slice().to_vec();
    let p = std::env::temp_dir().join(format!("ntn-neumann-{}.txt", std::process::id()));
    dump_restart(&a, &p).unwrap();
    load_restart(&mut a, &p).unwrap();
    assert_eq!(format!("{:?}", a.neumann_store().unwrap().forces()), record);
    assert_eq!(a.neumann_store().unwrap().stats(), stats);
    assert_eq!(a.external_force().unwrap().as_slice(), &f[..]);
    let mut b = model(read_mesh(&data("ntn_test_compression.msh"), 2), parser);
    b.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    load_restart(&mut b, &p).unwrap();
    assert!(b.neumann_store().is_none(), "nothing restored");
    assert!(
        b.external_force()
            .unwrap()
            .as_slice()
            .iter()
            .all(|v| *v == 0.0)
    );
    std::fs::remove_file(&p).ok();
}

/// The `text_all` dumper writes the requested fields on the contact nodes.
#[test]
fn contact_text_dumper() {
    use akantu_ntn_contact::ContactDumper;
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.1);
    let st = f.state(&mesh);
    c.compute_contact_pressure(&st).unwrap();
    let mut fr = allocate_friction(&c, mesh.nb_nodes(), "coulomb", "no_regularisation").unwrap();
    fr.set_param("mu", ParamValue::Real(0.2)).unwrap();
    fr.compute_friction_traction(&c, &st).unwrap();
    let dir = std::env::temp_dir().join(format!("ntn-dump-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut d = ContactDumper::new(&c);
    d.dumpable_mut().set_directory(&dir).unwrap();
    for name in [
        "velocity",
        "normal",
        "contact_pressure",
        "is_in_contact",
        "impedance",
        "lumped_boundary_master",
        "friction_traction",
        "is_sticking",
        "mu",
    ] {
        assert!(d.add_dump_field(name, &c, Some(&fr)).unwrap(), "{name}");
    }
    assert!(!d.add_dump_field("no_such_field", &c, Some(&fr)).unwrap());
    d.dump(&c, Some(&fr), &st, None, None).unwrap();
    let files = walk(&dir);
    let read = |name: &str| -> Vec<String> {
        let p = dir
            .join("contact-DataFiles")
            .join(format!("contact_{name}.out"));
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect()
    };
    // one line per contact node (3), the slaves' values
    assert_eq!(read("velocity"), vec!["1.000000e-01 -3.000000e-01"; 3]);
    assert_eq!(read("normal"), vec!["0.000000e+00 1.000000e+00"; 3]);
    assert_eq!(read("mu"), vec!["2.000000e-01"; 3]);
    assert_eq!(read("is_sticking"), vec!["0"; 3], "w > μ v: sliding");
    assert_eq!(read("is_in_contact"), vec!["1"; 3]);
    assert_eq!(read("lumped_boundary_master").len(), 3);
    assert_eq!(read("position").len(), 3);
    let fields = std::fs::read_to_string(dir.join("contact.fields")).unwrap();
    for name in [
        "contact_pressure",
        "friction_traction",
        "impedance",
        "normal",
        "velocity",
    ] {
        assert!(fields.contains(&format!("{name} N")), "{fields}");
    }
    assert!(!fields.contains("no_such_field"));
    assert!(files.len() >= 12);
    std::fs::remove_dir_all(&dir).ok();
}

fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// The state reads the model's release-cached current
/// position by reference (upstream `getCurrentPosition()` returns a
/// reference), it does not copy the nodal array on every call; the values
/// are those of the model (refreshed only on a release change).
#[test]
fn model_state_borrows_the_cached_current_position() {
    let mesh = read_mesh(&data("ntn_test_compression.msh"), 2);
    let parser = Parser::parse_file(data("material_ntn_model.dat")).unwrap();
    let mut model = model(mesh, parser);
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    let mesh_h = model.physics.base.mesh.clone();
    let m = mesh_h.read().unwrap();
    {
        let mut d_g = model.displacement_mut().unwrap();
        let d = d_g.as_mut_slice();
        for (i, v) in d.iter_mut().enumerate() {
            *v = 1e-6 * i as Real;
        }
    }
    let st = ModelState::of_model(&m, &mut model).unwrap();
    let cached = st
        .current_position
        .as_ref()
        .expect("cached current position");
    let cp = st.current_position().unwrap();
    assert_eq!(cp.as_ptr(), cached.as_ptr(), "borrowed, not copied");
    // release 0 == current-position release 0: still X (upstream)
    assert_eq!(&cp[..], m.nodes().as_slice());
}

/// The UD-039 domain error names its actual cause. A
/// law regular at rest (regularized Dieterich) still leaves its domain when
/// upstream's explicit-Euler ageing update θ ← θ(1 − VΔt/d_c) + Δt is
/// unstable (VΔt/d_c > 1): the state turns negative and ln(1 + θ/θ*) is NaN
/// at a sliding node; the message reports θ, V and VΔt/d_c and does not
/// blame a zero slip velocity.
#[test]
fn ud039_error_names_the_negative_state_of_an_unstable_update() {
    let mesh = mesh_2d();
    let mut f = Fields::new(mesh.nb_nodes());
    let mut c = ntn_contact(&mesh, &f, TasnUpstreamCompat::default());
    approach(&c, &mut f, 0.3, 0.0);
    c.compute_contact_pressure(&f.state(&mesh)).unwrap();
    let mut fr = allocate_friction(
        &c,
        mesh.nb_nodes(),
        "rate_and_state:regularized_dieterich:ageing",
        "no_regularisation",
    )
    .unwrap();
    let v = 1e-3;
    // V Δt / d_c = 3: θ₁ = θ₀ (1 − 3) + Δt = −1.9e-2 < −θ*
    let d_c = v * DT / 3.0;
    for (n, val) in [
        ("mu_0", 0.28),
        ("a", 0.005),
        ("b", 0.075),
        ("v_star", 1e-7),
        ("phi_star", 3.3e-4),
        ("d_c", d_c),
        ("friction_state", 1e-2),
    ] {
        fr.set_param(n, ParamValue::Real(val)).unwrap();
    }
    for i in 0..3 {
        fr.is_sticking_mut().set_at(i, false);
    }
    slip_by(&c, &mut f, v * DT);
    let st = f.state(&mesh);
    fr.update_slip(&c, &st).unwrap();
    match fr.compute_frictional_strength(&c, &st) {
        Err(Error::Numerical(m)) => {
            assert!(m.contains("UD-039"), "{m}");
            assert!(m.contains("negative state"), "{m}");
            assert!(m.contains("VΔt/d_c"), "{m}");
            assert!(!m.contains("zero slip velocity"), "{m}");
        }
        other => panic!("expected the UD-039 domain error, got {other:?}"),
    }
}
