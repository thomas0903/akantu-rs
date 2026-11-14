// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Spatial convergence
// study (no upstream counterpart): observed orders of convergence of the coupled solid /
// phase-field problem solved through CouplerSolidPhaseField
// (src/model/model_couplers/coupler_solid_phasefield.cc, with the solid's
// material_phasefield.cc and the phase field's phasefield_quadratic.cc).

//! Coupled spatial convergence through [`CouplerSolidPhaseField`] at fixed
//! `l0` (`docs/NUMERICAL_VALIDATION.md`, rule CONV-1 as amended). Generic harness:
//! `crates/akantu-fe/tests/convergence_common/mod.rs`; elastic part:
//! `crates/akantu-solid/tests/convergence_elastic/mod.rs`.
//!
//! The coupled static problem of the `phasefield` material (`no_split`,
//! `g(d) = (1 − d)²`) and the AT2 law (`history`, never saved, so `φ` is
//! the driving energy of the current strain):
//!
//! ```text
//! −div[g(d) C ε(u)] = b,
//! −div(g_c l0 ∇d) + (g_c/l0 + 2φ(ε(u))) d = 2φ(ε(u)) + f,   φ(ε) = ½ ε : C : ε,
//! ```
//!
//! with the manufactured pair `u*` (the displacement of the solid study)
//! and `d* = 0.2 + 0.5 Π_j sin(k_j x_j + c_j)`, the forces
//! `b = g(d*) b₀ + 2(1 − d*) σ₀(u*) ∇d*` (`b₀ = −div σ₀`,
//! `σ₀ = C ε(u*)`) and `f = −g_c l0 Δd* + (g_c/l0 + 2φ*) d* − 2φ*`
//! (`φ* = φ(ε(u*))`) applied as consistent loads of the test's own rule,
//! Dirichlet data `u*`, `d*` on the whole boundary. The staggered passes of
//! the coupler (`solve`: solid solve, strain transfer, phase-field solve,
//! damage transfer) are repeated to a fixed point (nodal increments below
//! `1e-12`). Parameters: `E = 10`, `ν = 0.3` (plane strain), `g_c = 0.05`,
//! `l0 = 0.5` (`2φ*` up to ≈ 22 % of `g_c/l0`: the strain really drives
//! the damage). Orders: displacement L2 `p + 1` and energy
//! `(∫ g(d*) ε(e) : C : ε(e))^½` `p`; damage L2 `p + 1` and energy
//! `(∫ g_c l0 |∇e|² + (g_c/l0 + 2φ*) e²)^½` `p`; the four 2-D types. The
//! finest level runs only in optimised builds (`cargo test --release -p
//! akantu-couplers --test spatial_convergence_phase_field`, a line of
//! `ci/tools/feature-gates.txt`).

#[path = "../../akantu-fe/tests/convergence_common/mod.rs"]
mod convergence_common;
#[path = "../../akantu-solid/tests/convergence_elastic/mod.rs"]
mod convergence_elastic;

use std::f64::consts::PI;
use std::time::Instant;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Real};
use akantu_couplers::CouplerSolidPhaseField;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use convergence_common::*;
use convergence_elastic::*;

use ElementType as E;

/// Points per direction of the test's own rule.
const M: usize = 5;

const YOUNG: Real = 10.0;
const NU: Real = 0.3;
const GC: Real = 0.05;
const L0: Real = 0.5;

/// Fixed-point tolerance of the staggered passes (largest nodal increment
/// of `u` relative to `max |u|`, and of `d`) and the pass limit.
const FIXED_POINT_TOL: Real = 1e-12;
const MAX_PASSES: usize = 200;

/// Levels as in the 2-D solid study (`N = n·p` from 8); the finest level
/// (≈ 30–40 s unoptimised: eleven staggered passes of two solves) only in
/// optimised builds, so the default profile asserts the pairs with bands
/// 1/8 and 1/16.
fn levels(ty: ElementType) -> Vec<usize> {
    let all = match order(ty) {
        1 => vec![8, 16, 32, 64],
        _ => vec![4, 8, 16, 32],
    };
    if cfg!(debug_assertions) {
        all[..all.len() - 1].to_vec()
    } else {
        all
    }
}

fn law(dim: usize) -> Elastic {
    Elastic {
        dim,
        e: YOUNG,
        nu: NU,
        rho: 1.0,
        plane_stress: false,
    }
}

fn dat() -> String {
    format!(
        "model solid_mechanics_model [\n  material phasefield no_split [\n    name = conv\n    \
         rho = 1\n    E = {YOUNG}\n    nu = {NU}\n    Plane_Stress = false\n  ]\n]\n\
         model phase_field_model [\n  phasefield quadratic no_split [\n    name = conv\n    \
         E = {YOUNG}\n    nu = {NU}\n    gc = {GC}\n    l0 = {L0}\n    Plane_Stress = false\n  ]\n]\n"
    )
}

/// `d* = 0.2 + 0.5 Π_j sin(k_j x_j + c_j)` (as the phase-field study).
#[derive(Clone, Copy, Debug)]
struct Damage {
    dim: usize,
}

const K: [Real; 3] = [0.8, 0.6, 0.7];
const PHASE: [Real; 3] = [0.3, 0.5, 0.4];

impl Damage {
    fn product(&self, x: &[Real]) -> (Real, [Real; 3]) {
        let mut s = 1.0;
        let mut g = [1.0; 3];
        for j in 0..self.dim {
            let k = K[j] * PI;
            let (sn, cs) = ((k * x[j] + PHASE[j]).sin(), (k * x[j] + PHASE[j]).cos());
            s *= sn;
            for (i, gi) in g.iter_mut().enumerate().take(self.dim) {
                *gi *= if i == j { k * cs } else { sn };
            }
        }
        (s, g)
    }
    fn value(&self, x: &[Real]) -> ([Real; 3], [[Real; 3]; 3]) {
        let (s, g) = self.product(x);
        let mut grad = [[0.0; 3]; 3];
        for j in 0..self.dim {
            grad[0][j] = 0.5 * g[j];
        }
        ([0.2 + 0.5 * s, 0.0, 0.0], grad)
    }
    fn laplacian(&self, x: &[Real]) -> Real {
        let k2: Real = (0..self.dim).map(|j| (K[j] * PI).powi(2)).sum();
        -k2 * 0.5 * self.product(x).0
    }
}

/// The manufactured coupled pair.
struct Coupled {
    dim: usize,
    disp: Manufactured,
    dam: Damage,
}

impl Coupled {
    fn new(dim: usize) -> Self {
        Self {
            dim,
            disp: Manufactured { dim, law: law(dim) },
            dam: Damage { dim },
        }
    }
    fn degradation(&self, x: &[Real]) -> Real {
        let d = self.dam.value(x).0[0];
        (1.0 - d) * (1.0 - d)
    }
    /// `φ* = ½ ε(u*) : C : ε(u*)`.
    fn phi(&self, x: &[Real]) -> Real {
        0.5 * self.disp.law.energy(&self.disp.grad(x))
    }
    /// `b = −div[g(d*) σ₀] = g b₀ − σ₀ ∇g`, `∇g = −2(1 − d*) ∇d*`.
    fn body_force(&self, x: &[Real]) -> [Real; 3] {
        let (dv, dg) = self.dam.value(x);
        let g = (1.0 - dv[0]) * (1.0 - dv[0]);
        let b0 = self.disp.body_force(x);
        let s0 = self.disp.law.stress(&self.disp.grad(x));
        let mut b = [0.0; 3];
        for i in 0..self.dim {
            b[i] = g * b0[i];
            for j in 0..self.dim {
                b[i] += 2.0 * (1.0 - dv[0]) * s0[i][j] * dg[0][j];
            }
        }
        b
    }
    /// `f = −g_c l0 Δd* + (g_c/l0 + 2φ*) d* − 2φ*`.
    fn damage_source(&self, x: &[Real]) -> [Real; 3] {
        let d = self.dam.value(x).0[0];
        let phi = self.phi(x);
        [
            -GC * L0 * self.dam.laplacian(x) + (GC / L0 + 2.0 * phi) * d - 2.0 * phi,
            0.0,
            0.0,
        ]
    }
}

/// One level: the two errors (displacement, damage) and the number of
/// staggered passes.
fn level(ty: ElementType, n: usize) -> ([Level; 2], usize) {
    let t0 = Instant::now();
    let dim = ty.spatial_dimension();
    let sm = structured_mesh(ty, n, true);
    let rd = RefData::new(ty, M);
    let ex = Coupled::new(dim);
    let parser = Parser::parse_str(&dat(), "material.dat").unwrap();
    let mut c = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(sm.to_mesh()),
        None,
        "coupler_solid_phasefield",
        parser,
    )
    .unwrap();
    c.solid
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    c.phase
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let nb = sm.nb_nodes();
    let mut blocked_u = vec![false; nb * dim];
    let mut u0 = vec![0.0; nb * dim];
    let mut blocked_d = vec![false; nb];
    let mut d0 = vec![0.0; nb];
    for node in 0..nb {
        if sm.on_boundary(node) {
            let x = &sm.nodes[node * dim..(node + 1) * dim];
            let ue = ex.disp.u(x);
            for i in 0..dim {
                blocked_u[node * dim + i] = true;
                u0[node * dim + i] = ue[i];
            }
            blocked_d[node] = true;
            d0[node] = ex.dam.value(x).0[0];
        }
    }
    let fu = field_load(&sm, &rd, dim, &|x| ex.body_force(x));
    let fd = field_load(&sm, &rd, 1, &|x| ex.damage_source(x));
    c.solid
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked_u);
    c.solid
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u0);
    c.solid
        .external_force_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&fu);
    c.phase
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked_d);
    c.phase
        .damage_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&d0);
    c.phase
        .external_force_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&fd);
    let mut u_old = u0;
    let mut d_old = d0;
    let mut passes = 0;
    loop {
        c.solve(None, None).unwrap();
        passes += 1;
        let u = c.solid.displacement().unwrap().as_slice().to_vec();
        let d = c.phase.damage().unwrap().as_slice().to_vec();
        let umax = u.iter().fold(0.0 as Real, |m, v| m.max(v.abs()));
        let du = u
            .iter()
            .zip(&u_old)
            .fold(0.0 as Real, |m, (a, b)| m.max((a - b).abs()));
        let dd = d
            .iter()
            .zip(&d_old)
            .fold(0.0 as Real, |m, (a, b)| m.max((a - b).abs()));
        u_old = u;
        d_old = d;
        if du <= FIXED_POINT_TOL * umax && dd <= FIXED_POINT_TOL {
            break;
        }
        assert!(
            passes < MAX_PASSES,
            "{ty} n={n}: no fixed point after {passes} passes (Δu {du:e}, Δd {dd:e})"
        );
    }
    let law = law(dim);
    let [ul2, uen, ul2n, uenn] = field_errors(
        &sm,
        &rd,
        &u_old,
        dim,
        &|x| (ex.disp.u(x), ex.disp.grad(x)),
        &|x, _, g| ex.degradation(x) * law.energy(g),
    );
    let [dl2, den, dl2n, denn] =
        field_errors(&sm, &rd, &d_old, 1, &|x| ex.dam.value(x), &|x, v, g| {
            let grad: Real = (0..dim).map(|j| g[0][j] * g[0][j]).sum();
            GC * L0 * grad + (GC / L0 + 2.0 * ex.phi(x)) * v[0] * v[0]
        });
    let seconds = t0.elapsed().as_secs_f64();
    (
        [
            Level {
                n,
                unknowns: nb * dim,
                l2: ul2,
                energy: uen,
                l2_rel: ul2 / ul2n,
                energy_rel: uen / uenn,
                seconds,
            },
            Level {
                n,
                unknowns: nb,
                l2: dl2,
                energy: den,
                l2_rel: dl2 / dl2n,
                energy_rel: den / denn,
                seconds,
            },
        ],
        passes,
    )
}

fn study(ty: ElementType) {
    let mut disp = Vec::new();
    let mut dam = Vec::new();
    for n in levels(ty) {
        let ([lu, ld], passes) = level(ty, n);
        eprintln!("coupled {ty} n={n}: {passes} staggered passes to the fixed point");
        disp.push(lu);
        dam.push(ld);
    }
    let p = order(ty) as Real;
    let mut bad = report(
        "coupled-displacement",
        ty,
        &disp,
        [p + 1.0, p],
        [EVERY_PAIR; 2],
    );
    bad.extend(report(
        "coupled-damage",
        ty,
        &dam,
        [p + 1.0, p],
        [EVERY_PAIR; 2],
    ));
    assert!(bad.is_empty(), "rule CONV-1 violated:\n{}", bad.join("\n"));
}

#[test]
fn coupled_triangle_3() {
    study(E::Triangle3);
}

#[test]
fn coupled_triangle_6() {
    study(E::Triangle6);
}

#[test]
fn coupled_quadrangle_4() {
    study(E::Quadrangle4);
}

#[test]
fn coupled_quadrangle_8() {
    study(E::Quadrangle8);
}

/// The forces of the manufactured pair: `b = −div[g(d*) C ε(u*)]` and the
/// damage equation residual vanishes with `f`, against central differences
/// (step 1e-4; truncation ≈ 1e-8 · |∂³(gσ)| and 1e-8 · |∂⁴d*|).
#[test]
fn coupled_forces_are_consistent() {
    let ex = Coupled::new(2);
    let x = [0.31, 0.67];
    let h = 1e-4;
    let b = ex.body_force(&x);
    for (i, bi) in b.iter().enumerate().take(2) {
        let mut div = 0.0;
        for j in 0..2 {
            let (mut xp, mut xm) = (x, x);
            xp[j] += h;
            xm[j] -= h;
            let sp = ex.disp.law.stress(&ex.disp.grad(&xp));
            let sm = ex.disp.law.stress(&ex.disp.grad(&xm));
            div += (ex.degradation(&xp) * sp[i][j] - ex.degradation(&xm) * sm[i][j]) / (2.0 * h);
        }
        assert!(
            (bi + div).abs() < 1e-7,
            "b_{i} = {bi} vs −div(gσ) = {}",
            -div
        );
    }
    let d = ex.dam.value(&x).0[0];
    let mut lap = 0.0;
    for j in 0..2 {
        let (mut xp, mut xm) = (x, x);
        xp[j] += h;
        xm[j] -= h;
        lap += (ex.dam.value(&xp).0[0] - 2.0 * d + ex.dam.value(&xm).0[0]) / (h * h);
    }
    let phi = ex.phi(&x);
    let r = -GC * L0 * lap + (GC / L0 + 2.0 * phi) * d - 2.0 * phi - ex.damage_source(&x)[0];
    assert!(r.abs() < 1e-8, "damage equation residual {r}");
    // the strain drives the damage: 2φ* is a sizeable part of g_c/l0
    let mut largest = 0.0 as Real;
    for i in 0..=20 {
        for j in 0..=20 {
            let p = [i as Real / 20.0, j as Real / 20.0];
            largest = largest.max(2.0 * ex.phi(&p) / (GC / L0));
        }
    }
    eprintln!("max 2φ*/(g_c/l0) on a 21 × 21 grid: {largest:.3}");
    assert!(largest > 0.1, "2φ*/(g_c/l0) only {largest}");
}
