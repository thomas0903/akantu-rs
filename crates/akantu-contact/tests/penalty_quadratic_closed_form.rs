// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (CONT-016). Closed forms
// read from Akantu (EPFL/LSMS)
// src/model/contact_mechanics/resolutions/resolution_penalty_quadratic.cc
// (`computeNormalTraction`, `computeNormalModuli`) and resolution_penalty.cc
// (`computeNormalForce`).
//! `penalty_quadratic` pinned by closed forms (the finite-difference check of
//! `contact_stiffness.rs` passes if traction and derivative change together):
//!
//! * the pressure law `p = ε_n(⟨g⟩² + ⟨g⟩)` and its derivative;
//! * the local normal force of one contact element, `f = p A_s Aᵀn`;
//! * the normal moduli of upstream's expression (UD-368 compat) on a flat
//!   master facet, with upstream's rotational dyad `n ⊗ τ` in both terms
//!   (UD-008, `quadratic_rotational_dyad = true`: asymmetric) and with the
//!   corrected `τ ⊗ n` (`false`: symmetric).

mod common;

use akantu_contact::{
    ContactElement, ContactMechanicsModel, ContactUpstreamCompat, PenaltyLaw, ResolutionPenalty,
};
use akantu_core::Real;
use common::*;

const EPS_N: Real = 1.0e3;
const GAP: Real = 0.02;

/// `p(g)` closed form.
fn pressure(g: Real) -> Real {
    let m = g.max(0.0);
    EPS_N * (m * m + m)
}

#[test]
fn pressure_law_and_derivative_closed_form() {
    let mut r = ResolutionPenalty::new("m:0:penalty_quadratic", PenaltyLaw::Quadratic);
    r.epsilon_n = EPS_N;
    for (g, p, dp) in [
        (-0.1, 0.0, 0.0),
        (0.0, 0.0, EPS_N), // heaviside(0) = 1: the derivative at 0 is ε_n
        (0.02, EPS_N * (0.0004 + 0.02), EPS_N * 1.04),
        (1.0, 2.0 * EPS_N, 3.0 * EPS_N),
    ] {
        assert!(
            (r.normal_traction(g) - p).abs() <= 1e-12 * p.abs().max(1.0),
            "p({g})"
        );
        assert!(
            (r.normal_traction_derivative(g) - dp).abs() <= 1e-12 * dp.abs().max(1.0),
            "p'({g})"
        );
    }
    // the linear law differs from the quadratic one by exactly ε_n⟨g⟩²
    let mut l = ResolutionPenalty::new("m:0:penalty_linear", PenaltyLaw::Linear);
    l.epsilon_n = EPS_N;
    for g in [-0.3, 0.0, 0.05, 0.7] {
        let d = r.normal_traction(g) - l.normal_traction(g);
        assert!((d - EPS_N * g.max(0.0).powi(2)).abs() <= 1e-12 * EPS_N);
    }
}

/// Flat horizontal master facets, the upper body pushed `GAP` into them.
/// Returns the model, the contact element of the slave at x = 0.3 (inside
/// the master facet `[0, 0.5]`, ξ = 0.2) and the facet's data.
struct Setup {
    model: ContactMechanicsModel,
    ce: ContactElement,
    /// Shape functions of the two master nodes at the projection.
    n: [Real; 2],
    area: Real,
}

fn setup(compat: ContactUpstreamCompat) -> Setup {
    let mesh = two_blocks_2d(4, 1.0, 1, 0.3);
    let dat = contact_dat("master", "slave", 0.0, EPS_N, 0.0, true)
        .replace("penalty_linear", "penalty_quadratic");
    let (shared, mut model) = contact_model(mesh, &dat);
    model.set_upstream_compat(compat);
    translate_group(&mut model, &shared, "upper_body", &[0.0, -GAP]);
    model.search().unwrap();
    // scoped guards (storage-migration guide, P12): only plain values leave
    let (slave, ce, xs, x1, x2) = {
        let m = shared.read().unwrap();
        let slave = nodes_of_physical(&m, "slave")
            .into_iter()
            .find(|&n| (model.positions().tuple(n as usize)[0] - 0.3).abs() < 1e-12)
            .expect("slave at x = 0.3");
        let ce = *model
            .contact_elements()
            .iter()
            .find(|c| c.slave == slave)
            .expect("contact element of the slave at x = 0.3");
        let (c1, c2) = {
            let conn = m.element_connectivity(ce.master).unwrap();
            (conn[0], conn[1])
        };
        let x = |n: i32| model.positions().tuple(n as usize)[0];
        (slave, ce, x(slave), x(c1), x(c2))
    };
    assert!((x2 - x1).abs() > 0.4 && xs > x1.min(x2) && xs < x1.max(x2));
    let n = [(x2 - xs) / (x2 - x1), (xs - x1) / (x2 - x1)];
    let area = model.nodal_area().tuple(slave as usize)[0];
    // the geometry assumed by the closed forms
    let nrm = model.normals().tuple(slave as usize).to_vec();
    let tan = model.fields.tangents.tuple(slave as usize).to_vec();
    assert!((model.gaps().tuple(slave as usize)[0] - GAP).abs() < 1e-12);
    assert!(
        nrm[0].abs() < 1e-12 && (nrm[1] - 1.0).abs() < 1e-12,
        "{nrm:?}"
    );
    assert!(
        (tan[0] - 1.0).abs() < 1e-12 && tan[1].abs() < 1e-12,
        "{tan:?}"
    );
    assert!(area > 0.0);
    Setup { model, ce, n, area }
}

/// `f = ε_n(g² + g) A_s Aᵀ n` with `A = [I, −N₁ I, −N₂ I]`, `n = (0, 1)`:
/// the slave takes `(0, p)`, the master nodes `(0, −N_i p)`.
#[test]
fn one_element_normal_force_closed_form() {
    let mut s = setup(ContactUpstreamCompat::default());
    let ce = s.ce;
    let (f, ft) = s.model.local_forces(0, &ce).unwrap();
    let p = pressure(GAP) * s.area;
    let want = [0.0, p, 0.0, -s.n[0] * p, 0.0, -s.n[1] * p];
    assert_eq!(f.len(), 6);
    for (i, (a, b)) in f.iter().zip(want).enumerate() {
        // the projection is Newton-iterated to 1e-10 (projection_tolerance
        // of the input), which bounds the shape functions
        assert!((a - b).abs() <= 1e-9 * p, "f[{i}] = {a}, want {b}");
    }
    // frictionless: no tangential force
    assert!(ft.iter().all(|v| v.abs() < 1e-12), "{ft:?}");
    // equilibrium of the local force (action = −reaction)
    assert!(f.iter().skip(1).step_by(2).sum::<Real>().abs() <= 1e-9 * p);
}

type Mat = Vec<Vec<Real>>;

fn matmul(a: &Mat, b: &Mat) -> Mat {
    let (n, k, m) = (a.len(), b.len(), b[0].len());
    let mut c = vec![vec![0.0; m]; n];
    for i in 0..n {
        for j in 0..m {
            for l in 0..k {
                c[i][j] += a[i][l] * b[l][j];
            }
        }
    }
    c
}

fn transpose(a: &Mat) -> Mat {
    (0..a[0].len())
        .map(|j| a.iter().map(|r| r[j]).collect())
        .collect()
}

/// `a bᵀ` of two 2-vectors.
fn outer(a: [Real; 2], b: [Real; 2]) -> Mat {
    (0..2)
        .map(|i| (0..2).map(|j| a[i] * b[j]).collect())
        .collect()
}

/// Upstream's `K = k_main + k_rot1 + k_rot2` of `penalty_quadratic`
/// (`computeNormalModuli`) for the flat 2-D facet: `A = [I, −N₁I, −N₂I]`,
/// `A₁ = [0, ∂N₁/∂ξ I, ∂N₂/∂ξ I]` with `∂N/∂ξ = (−½, ½)`, `n = (0, 1)`,
/// `τ = (1, 0)` (unit tangent, contravariant metric `a¹¹ = 1`):
/// `k_main = ε_n(2g + 1)A_s Aᵀ(n nᵀ)A`,
/// `k_rot1 = −ε_n(g² + g)A_s A₁ᵀ(n τᵀ)A`,
/// `k_rot2 = −ε_n(g² + g)A_s Aᵀ X A₁` with `X = n τᵀ` (upstream) or `τ nᵀ`.
fn expected_moduli(s: &Setup, upstream_dyad: bool) -> Mat {
    let [n1, n2] = s.n;
    let a: Mat = vec![
        vec![1.0, 0.0, -n1, 0.0, -n2, 0.0],
        vec![0.0, 1.0, 0.0, -n1, 0.0, -n2],
    ];
    let a1: Mat = vec![
        vec![0.0, 0.0, -0.5, 0.0, 0.5, 0.0],
        vec![0.0, 0.0, 0.0, -0.5, 0.0, 0.5],
    ];
    let (n, t) = ([0.0, 1.0], [1.0, 0.0]);
    let at = transpose(&a);
    let main = matmul(&matmul(&at, &outer(n, n)), &a);
    let rot1 = matmul(&matmul(&transpose(&a1), &outer(n, t)), &a);
    let x = if upstream_dyad {
        outer(n, t)
    } else {
        outer(t, n)
    };
    let rot2 = matmul(&matmul(&at, &x), &a1);
    let k_main = EPS_N * (2.0 * GAP + 1.0) * s.area;
    let k_rot = -EPS_N * (GAP * GAP + GAP) * s.area;
    (0..6)
        .map(|i| {
            (0..6)
                .map(|j| k_main * main[i][j] + k_rot * (rot1[i][j] + rot2[i][j]))
                .collect()
        })
        .collect()
}

fn asymmetry(k: &[Real]) -> Real {
    let nd = 6;
    let mut d: Real = 0.0;
    for i in 0..nd {
        for j in 0..nd {
            d = d.max((k[i + j * nd] - k[j + i * nd]).abs());
        }
    }
    d
}

/// Upstream's normal moduli (UD-368 compat) with both settings of the UD-008
/// switch, against the closed form of the C++ expression.
#[test]
fn normal_moduli_closed_form_in_both_dyad_modes() {
    let mut asym = [0.0; 2];
    for (idx, dyad) in [true, false].into_iter().enumerate() {
        let compat = ContactUpstreamCompat {
            normal_moduli: true,
            quadratic_rotational_dyad: dyad,
            ..ContactUpstreamCompat::default()
        };
        let mut s = setup(compat);
        let ce = s.ce;
        let (k, _) = s.model.local_moduli(0, &ce).unwrap();
        let want = expected_moduli(&s, dyad);
        let scale = EPS_N * s.area;
        for i in 0..6 {
            for j in 0..6 {
                let got = k[i + j * 6];
                assert!(
                    (got - want[i][j]).abs() <= 1e-9 * scale,
                    "dyad = {dyad}: K[{i}][{j}] = {got}, closed form {}",
                    want[i][j]
                );
            }
        }
        asym[idx] = asymmetry(&k);
        // the two modes must differ in the closed form too (otherwise the
        // comparison above would not pin the switch)
        let other = expected_moduli(&s, !dyad);
        let d: Real = (0..6)
            .flat_map(|i| (0..6).map(move |j| (i, j)))
            .map(|(i, j)| (want[i][j] - other[i][j]).abs())
            .fold(0.0, Real::max);
        assert!(d > 1e-3 * scale, "the dyad switch changes K by {d}");
    }
    // upstream's dyad makes the quadratic moduli asymmetric (UD-008), the
    // corrected one keeps them symmetric
    let scale = EPS_N;
    assert!(
        asym[0] > 1e-3 * scale,
        "upstream dyad asymmetry {}",
        asym[0]
    );
    assert!(
        asym[1] < 1e-9 * scale,
        "corrected dyad asymmetry {}",
        asym[1]
    );
}
