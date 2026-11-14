// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Spatial
// convergence study (no upstream counterpart):
// observed orders of convergence of the solid mechanics model
// (src/model/solid_mechanics/solid_mechanics_model.cc, materials/
// material_elastic.cc, the element classes of src/fe_engine/
// element_classes/* and the Gauss rules of gauss_integration.cc) under
// uniform refinement.

//! Spatial convergence of the solid mechanics model
//! (`docs/NUMERICAL_VALIDATION.md`, rule CONV-1). Support modules: the
//! generic harness
//! `crates/akantu-fe/tests/convergence_common/mod.rs` (shared with the
//! diffusion, phase-field and coupler studies) and the elastic
//! part `tests/convergence_elastic/mod.rs`.
//!
//! * **Static** linear elasticity, every regular element type (12): the
//!   manufactured field `u_i = A_i Π_j sin(k_ij x_j + c_ij)` with Dirichlet
//!   data `u` on the whole boundary and the consistent load of
//!   `b = −div σ(u)`, on `n^d` cells of the unit cube mapped by a smooth
//!   distortion (every node, so the second-order elements are curved
//!   isoparametric ones); errors in L2 (expected order `p + 1`) and in the
//!   energy norm `(∫ ε : C : ε)^½` (expected `p`), `p` the order of the
//!   largest complete polynomial space of the interpolation.
//! * **Plane stress** (`Plane_Stress = true`), the four 2-D types.
//! * **Explicit** central differences with lumped mass: a standing P-wave
//!   with sliding walls (closed form), Δt ∝ h at a fixed CFL number: L2
//!   error second order in h and Δt together, energy error first order on
//!   the pairs with `n ≥ 16`.
//!
//! The stiffness is the library's (default Gauss rules); the load vector,
//! the geometry of the error integrals and the errors use the test's own
//! Gauss–Legendre / collapsed rules and derivatives of `N` by central
//! differences, so a defect of the library's rules or `dN/dξ` changes the
//! measured rates. Band (rule CONV-1): `|r − r_th| ≤ 1/N` for every pair
//! of levels `n → 2n`, `N = n·p`. The finest 3-D level runs only in
//! optimised builds (`cargo test --release -p akantu-solid --test
//! spatial_convergence`, a line of `ci/tools/feature-gates.txt`); the
//! default profile asserts the coarser pairs with the same rule.

#[path = "../../akantu-fe/tests/convergence_common/mod.rs"]
mod convergence_common;
mod convergence_elastic;

use std::f64::consts::PI;
use std::time::Instant;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Real};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use convergence_common::*;
use convergence_elastic::*;

use ElementType as E;

/// Points per direction of the test's own rule (exact for total degree 7
/// on every reference shape, `reference_rule_is_exact`).
const M: usize = 5;

/// Refinement levels (cells per direction), each twice the previous one.
/// 1-D and 2-D start at `N = n·p = 8` nodal intervals per unit length
/// (≈ 12 nodes per shortest wavelength `2/1.3` of the field); 3-D starts
/// at `n = 2` (cost of the direct solver in unoptimised builds: ≈ 3 s at
/// 2 187 unknowns, ≈ 60 s at 8 019) and its finest level runs only in
/// optimised builds.
fn levels(ty: ElementType) -> Vec<usize> {
    let all = match (ty.spatial_dimension(), order(ty)) {
        (1 | 2, 1) => vec![8, 16, 32, 64],
        (1 | 2, _) => vec![4, 8, 16, 32],
        (_, 1) => vec![2, 4, 8, 16],
        _ => vec![2, 4, 8],
    };
    if ty.spatial_dimension() == 3 && cfg!(debug_assertions) {
        all[..all.len() - 1].to_vec()
    } else {
        all
    }
}

fn law(dim: usize, plane_stress: bool) -> Elastic {
    Elastic {
        dim,
        e: 1.0,
        nu: 0.3,
        rho: 1.0,
        plane_stress,
    }
}

fn model_on(sm: &StructuredMesh, law: &Elastic, method: AnalysisMethod) -> SolidMechanicsModel {
    let parser = Parser::parse_str(&law.dat(), "material.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(sm.to_mesh()),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    model.init_full(&ModelOptions::new(method)).unwrap();
    model
}

/* ---------------------------------------------------------------------- */
/* Static manufactured solution                                            */
/* ---------------------------------------------------------------------- */

/// Static solve on `sm`: Dirichlet data `u_bc` on the whole boundary, nodal
/// external force `load`; returns the nodal displacement.
fn static_solve(
    sm: &StructuredMesh,
    law: &Elastic,
    u_bc: &dyn Fn(&[Real]) -> [Real; 3],
    load: &[Real],
) -> Vec<Real> {
    let dim = sm.dim;
    let mut model = model_on(sm, law, AnalysisMethod::Static);
    let mut blocked = vec![false; sm.nb_nodes() * dim];
    let mut u0 = vec![0.0; sm.nb_nodes() * dim];
    for node in 0..sm.nb_nodes() {
        if sm.on_boundary(node) {
            let ue = u_bc(&sm.nodes[node * dim..(node + 1) * dim]);
            for i in 0..dim {
                blocked[node * dim + i] = true;
                u0[node * dim + i] = ue[i];
            }
        }
    }
    model
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked);
    model
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u0);
    model
        .external_force_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(load);
    model.solve_step(None).unwrap();
    model.displacement().unwrap().as_slice().to_vec()
}

/// Static manufactured problem on the `n`-level mesh of `ty`: Dirichlet data
/// `u` on the whole boundary, consistent load of `b = −div σ(u)`.
fn static_level(ty: ElementType, n: usize, law: &Elastic) -> Level {
    let t0 = Instant::now();
    let dim = ty.spatial_dimension();
    let sm = structured_mesh(ty, n, true);
    let exact = Manufactured { dim, law: *law };
    let rd = RefData::new(ty, M);
    let load = nodal_load(&sm, &rd, &|x| exact.body_force(x));
    let u = static_solve(&sm, law, &|x| exact.u(x), &load);
    let [l2, energy, l2u, eu] = errors(&sm, &rd, &u, &|x| (exact.u(x), exact.grad(x)), law);
    Level {
        n,
        unknowns: sm.nb_nodes() * dim,
        l2,
        energy,
        l2_rel: l2 / l2u,
        energy_rel: energy / eu,
        seconds: t0.elapsed().as_secs_f64(),
    }
}

fn static_study(ty: ElementType, plane_stress: bool) {
    let law = law(ty.spatial_dimension(), plane_stress);
    let lv = levels(ty);
    let res: Vec<Level> = lv.iter().map(|&n| static_level(ty, n, &law)).collect();
    let p = order(ty) as Real;
    let tag = if plane_stress {
        "plane-stress"
    } else {
        "static"
    };
    let bad = report(tag, ty, &res, [p + 1.0, p], [EVERY_PAIR; 2]);
    assert!(bad.is_empty(), "rule CONV-1 violated:\n{}", bad.join("\n"));
}

#[test]
fn static_segment_2() {
    static_study(E::Segment2, false);
}

#[test]
fn static_segment_3() {
    static_study(E::Segment3, false);
}

#[test]
fn static_triangle_3() {
    static_study(E::Triangle3, false);
}

#[test]
fn static_triangle_6() {
    static_study(E::Triangle6, false);
}

#[test]
fn static_quadrangle_4() {
    static_study(E::Quadrangle4, false);
}

#[test]
fn static_quadrangle_8() {
    static_study(E::Quadrangle8, false);
}

#[test]
fn static_tetrahedron_4() {
    static_study(E::Tetrahedron4, false);
}

#[test]
fn static_tetrahedron_10() {
    static_study(E::Tetrahedron10, false);
}

#[test]
fn static_pentahedron_6() {
    static_study(E::Pentahedron6, false);
}

#[test]
fn static_pentahedron_15() {
    static_study(E::Pentahedron15, false);
}

#[test]
fn static_hexahedron_8() {
    static_study(E::Hexahedron8, false);
}

#[test]
fn static_hexahedron_20() {
    static_study(E::Hexahedron20, false);
}

/// The 2-D law with `Plane_Stress = true` (upstream `PlaneStressToolbox`:
/// `λ* = νE / ((1 + ν)(1 − ν))`), same field and levels.
#[test]
fn plane_stress_triangle_3() {
    static_study(E::Triangle3, true);
}

#[test]
fn plane_stress_triangle_6() {
    static_study(E::Triangle6, true);
}

#[test]
fn plane_stress_quadrangle_4() {
    static_study(E::Quadrangle4, true);
}

#[test]
fn plane_stress_quadrangle_8() {
    static_study(E::Quadrangle8, true);
}

/* ---------------------------------------------------------------------- */
/* Explicit standing wave                                                   */
/* ---------------------------------------------------------------------- */

/// Standing P-wave of the unit square / cube with sliding walls (normal
/// displacement blocked, tangential traction free): `u = A ∇φ cos(ωt)`,
/// `φ = Π_j cos(m_j π x_j)` (up to sign), `ω = π |m| c_P`,
/// `c_P = √((λ + 2μ)/ρ)`. On a wall `x_j ∈ {0, 1}` the normal component
/// `∝ sin(m_j π x_j)` and the shear stress vanish, so this is an exact
/// solution of the Navier equations with these boundary conditions.
struct StandingWave {
    dim: usize,
    law: Elastic,
    m: [Real; 3],
    amp: Real,
}

impl StandingWave {
    fn c_p(&self) -> Real {
        ((self.law.lambda() + 2.0 * self.law.mu()) / self.law.rho).sqrt()
    }
    fn omega(&self) -> Real {
        let k2: Real = (0..self.dim).map(|j| (self.m[j] * PI).powi(2)).sum();
        k2.sqrt() * self.c_p()
    }
    /// `u(x, t)` and `∇u(x, t)`.
    fn at(&self, x: &[Real], t: Real) -> ([Real; 3], [[Real; 3]; 3]) {
        let d = self.dim;
        let c = (self.omega() * t).cos() * self.amp;
        let k: Vec<Real> = (0..d).map(|j| self.m[j] * PI).collect();
        let cs: Vec<Real> = (0..d).map(|j| (k[j] * x[j]).cos()).collect();
        let sn: Vec<Real> = (0..d).map(|j| (k[j] * x[j]).sin()).collect();
        let mut u = [0.0; 3];
        let mut g = [[0.0; 3]; 3];
        // u_i = k_i f_i(x_i) Π_{j≠i} cos(k_j x_j) with f_i = sin(k_i ·), and
        // ∂_l u_i: the factor of x_l differentiated (sin' = k cos, cos' = −k sin)
        let factor = |i: usize, j: usize, l: Option<usize>| -> Real {
            match (j == i, l == Some(j)) {
                (true, true) => k[j] * cs[j],
                (true, false) => sn[j],
                (false, true) => -k[j] * sn[j],
                (false, false) => cs[j],
            }
        };
        for i in 0..d {
            u[i] = c * (0..d).fold(k[i], |v, j| v * factor(i, j, None));
            for (l, gil) in g[i].iter_mut().enumerate().take(d) {
                *gil = c * (0..d).fold(k[i], |w, j| w * factor(i, j, Some(l)));
            }
        }
        (u, g)
    }
}

/// Central differences with lumped mass (`AnalysisMethod::ExplicitLumpedMass`)
/// from `u(·, 0)`, `v = 0` and the consistent initial acceleration
/// `a₀ = M⁻¹ (f_ext + f_int(u₀))` (the model starts from the acceleration
/// it is given, upstream likewise; with `a₀ = 0` the first velocity
/// half-step misses `Δt/2 · a₀` and the scheme is first order in time), to
/// `t_end` in `steps_per_n · n` steps (`consistent_a0 = false`: from
/// `a = 0`, the negative control); errors are the maxima over the
/// times `t_end/4, t_end/2, 3t_end/4, t_end`. Returns the level and
/// `(Δt, stable Δt of the model)`.
fn wave_level(
    ty: ElementType,
    n: usize,
    wave: &StandingWave,
    t_end: Real,
    steps_per_n: usize,
    consistent_a0: bool,
) -> (Level, [Real; 2]) {
    let t0 = Instant::now();
    let dim = ty.spatial_dimension();
    let sm = structured_mesh(ty, n, true);
    let rd = RefData::new(ty, M);
    let mut model = model_on(&sm, &wave.law, AnalysisMethod::ExplicitLumpedMass);
    let nb = sm.nb_nodes() * dim;
    let mut blocked = vec![false; nb];
    let mut u0 = vec![0.0; nb];
    for node in 0..sm.nb_nodes() {
        let (ue, _) = wave.at(&sm.nodes[node * dim..(node + 1) * dim], 0.0);
        for i in 0..dim {
            blocked[node * dim + i] = sm.faces[node][i] != 0;
            u0[node * dim + i] = ue[i];
        }
    }
    model
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked);
    model
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u0);
    let steps = steps_per_n * n;
    assert_eq!(steps % 4, 0);
    let dt = t_end / steps as Real;
    let dt_stable = model.stable_time_step().unwrap();
    model.set_time_step(dt, None).unwrap();
    model.assemble_internal_forces().unwrap();
    model.assemble_mass_lumped().unwrap();
    let a0: Vec<Real> = {
        let fi = model.internal_force().unwrap().as_slice().to_vec();
        let fe = model.external_force().unwrap().as_slice().to_vec();
        let m = model.mass().unwrap().as_slice().to_vec();
        (0..nb)
            .map(|k| {
                if blocked[k] {
                    0.0
                } else {
                    (fe[k] + fi[k]) / m[k]
                }
            })
            .collect()
    };
    if consistent_a0 {
        model
            .acceleration_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&a0);
    }
    let (mut l2, mut energy, mut l2u, mut eu) =
        (0.0 as Real, 0.0 as Real, 0.0 as Real, 0.0 as Real);
    for s in 1..=steps {
        model.solve_step(None).unwrap();
        if (4 * s) % steps == 0 {
            let t = s as Real * dt;
            let u = model.displacement().unwrap().as_slice().to_vec();
            let [e0, e1, n0, n1] = errors(&sm, &rd, &u, &|x| wave.at(x, t), &wave.law);
            l2 = l2.max(e0);
            energy = energy.max(e1);
            l2u = l2u.max(n0);
            eu = eu.max(n1);
        }
    }
    let level = Level {
        n,
        unknowns: nb,
        l2,
        energy,
        l2_rel: l2 / l2u,
        energy_rel: energy / eu,
        seconds: t0.elapsed().as_secs_f64(),
    };
    (level, [dt, dt_stable])
}

/// Mode `(2, 1)` of the unit square in plane strain, `t_end = 1`
/// (`ωt_end ≈ 8.15`, 1.3 periods), `Δt = t_end / (3n)`: CFL number
/// `c_P Δt / h = c_P / 3 ≈ 0.39` on every level (`h = 1/n`).
fn wave_levels(ty: ElementType, consistent_a0: bool) -> Vec<Level> {
    let wave = StandingWave {
        dim: 2,
        law: law(2, false),
        m: [2.0, 1.0, 0.0],
        amp: 0.01,
    };
    let lv: Vec<usize> = vec![8, 16, 32, 64];
    let mut res = Vec::new();
    for &n in &lv {
        let (level, [dt, stable]) = wave_level(ty, n, &wave, 1.0, 3, consistent_a0);
        eprintln!(
            "explicit {ty} n={n}: Δt = {dt:.4e}, model stable Δt = {stable:.4e} (ratio {:.3}), CFL c_P Δt n = {:.3}",
            dt / stable,
            wave.c_p() * dt * n as Real
        );
        assert!(
            dt < stable,
            "{ty} n={n}: Δt {dt} above the stable time step {stable}"
        );
        res.push(level);
    }
    res
}

fn wave_study(ty: ElementType) {
    let res = wave_levels(ty, true);
    // the energy-norm error mixes the O(h) interpolation error with the
    // O(h²) phase error of constant ∝ ω t_end: first order only
    // asymptotically, so it is asserted on the pairs with n ≥ 16 and the
    // coarse pair n = 8 → 16 is reported only
    // (NUMERICAL_VALIDATION)
    let bad = report("explicit", ty, &res, [2.0, 1.0], [EVERY_PAIR, Some(16)]);
    assert!(bad.is_empty(), "rule CONV-1 violated:\n{}", bad.join("\n"));
}

#[test]
fn explicit_standing_wave_quadrangle_4() {
    wave_study(E::Quadrangle4);
}

#[test]
fn explicit_standing_wave_triangle_3() {
    wave_study(E::Triangle3);
}

/// Negative control: the same `_quadrangle_4` run started from `a = 0`
/// (first-order start, see `wave_level`) must violate rule CONV-1 at the
/// finest pair, i.e. the fixed-CFL study does see the temporal order
/// (measured L2 orders 1.54, 1.43, 1.28).
#[test]
fn explicit_standing_wave_detects_a_first_order_start() {
    let res = wave_levels(E::Quadrangle4, false);
    let bad = report(
        "explicit-a0=0",
        E::Quadrangle4,
        &res,
        [2.0, 1.0],
        [EVERY_PAIR, None],
    );
    assert!(
        bad.iter().any(|b| b.contains("for n = 32 → 64")),
        "a first-order start was not detected at the finest pair: {bad:?}"
    );
}

/* ---------------------------------------------------------------------- */
/* Checks of the test's own machinery                                      */
/* ---------------------------------------------------------------------- */

/// The test's rule integrates monomials `ξ^a η^b ζ^c` of total degree
/// `≤ 2M − 3 = 7` exactly on every reference shape (each collapsed
/// direction adds one degree in the Duffy variable: `2M − 1` per Gauss
/// direction, minus 2 for the tetrahedron; closed forms `∫_{[-1,1]} ξ^a`,
/// `a! b! / (a + b + 2)!`, `a! b! c! / (a + b + c + 3)!`).
#[test]
fn reference_rule_is_exact() {
    let fact = |k: u32| -> Real { (1..=k).map(|i| i as Real).product() };
    let seg = |a: u32| {
        if a % 2 == 1 {
            0.0
        } else {
            2.0 / (a as Real + 1.0)
        }
    };
    for shape in [
        RefShape::Cube(1),
        RefShape::Cube(2),
        RefShape::Cube(3),
        RefShape::Triangle,
        RefShape::Tetrahedron,
        RefShape::Prism,
    ] {
        let (pts, w) = ref_rule(shape, M);
        let total: Real = w.iter().sum();
        assert!(
            (total - ref_measure(shape)).abs() < 1e-14,
            "{shape:?}: Σw = {total}"
        );
        let deg = 2 * M as u32 - 3;
        for a in 0..=deg {
            for b in 0..=(deg - a) {
                for c in 0..=(deg - a - b) {
                    let exact = match shape {
                        RefShape::Cube(1) if b + c == 0 => seg(a),
                        RefShape::Cube(2) if c == 0 => seg(a) * seg(b),
                        RefShape::Cube(3) => seg(a) * seg(b) * seg(c),
                        RefShape::Triangle if c == 0 => fact(a) * fact(b) / fact(a + b + 2),
                        RefShape::Tetrahedron => fact(a) * fact(b) * fact(c) / fact(a + b + c + 3),
                        RefShape::Prism => seg(a) * fact(b) * fact(c) / fact(b + c + 2),
                        _ => continue,
                    };
                    let q: Real = pts
                        .iter()
                        .zip(&w)
                        .map(|(p, w)| {
                            w * p[0].powi(a as i32) * p[1].powi(b as i32) * p[2].powi(c as i32)
                        })
                        .sum();
                    assert!(
                        (q - exact).abs() < 1e-14,
                        "{shape:?} ξ^{a} η^{b} ζ^{c}: {q} vs {exact}"
                    );
                }
            }
        }
    }
}

/// The library's `dN/dξ` equals the central differences of its `N` (exact
/// for these shape functions, see `shapes_at`) at the points of the rule,
/// which justifies measuring the errors with the differences.
#[test]
fn library_dnds_are_the_derivatives_of_n() {
    for ty in ALL_TYPES {
        let (pts, _) = ref_rule(ref_shape(ty), M);
        let (_, lib) = library_shapes(ty, &pts);
        let (_, fd) = shapes_at(ty, &pts);
        let worst = lib
            .iter()
            .zip(&fd)
            .fold(0.0 as Real, |m, (a, b)| m.max((a - b).abs()));
        assert!(worst < 1e-10, "{ty}: max |dN/dξ − FD| = {worst:e}");
    }
}

/// `b = −div σ(u)` of the manufactured field against central differences of
/// the stress (step 1e-4: truncation ≈ 1e-8 · |σ'''|).
#[test]
fn body_force_is_minus_div_sigma() {
    for (dim, ps) in [(1, false), (2, false), (2, true), (3, false)] {
        let law = law(dim, ps);
        let ex = Manufactured { dim, law };
        let x = [0.31, 0.67, 0.43];
        let b = ex.body_force(&x[..dim]);
        let h = 1e-4;
        for i in 0..dim {
            let mut div = 0.0;
            for j in 0..dim {
                let (mut xp, mut xm) = (x, x);
                xp[j] += h;
                xm[j] -= h;
                let sp = law.stress(&ex.grad(&xp[..dim]));
                let sm = law.stress(&ex.grad(&xm[..dim]));
                div += (sp[i][j] - sm[i][j]) / (2.0 * h);
            }
            assert!(
                (b[i] + div).abs() < 1e-8,
                "dim {dim} ps {ps}: b_{i} = {} vs −div σ = {}",
                b[i],
                -div
            );
        }
    }
}

/// The standing wave solves the Navier equations: `ρ ü = div σ` against
/// central differences in space and time, and the walls are sliding
/// (`u_n = 0`, `σ_nt = 0`).
#[test]
fn standing_wave_is_an_exact_solution() {
    let law = law(2, false);
    let wave = StandingWave {
        dim: 2,
        law,
        m: [2.0, 1.0, 0.0],
        amp: 0.01,
    };
    let (x, t, h) = ([0.37, 0.61, 0.0], 0.29, 1e-4);
    let (up, _) = wave.at(&x[..2], t + h);
    let (u0, _) = wave.at(&x[..2], t);
    let (um, _) = wave.at(&x[..2], t - h);
    for i in 0..2 {
        let acc = (up[i] - 2.0 * u0[i] + um[i]) / (h * h);
        let mut div = 0.0;
        for j in 0..2 {
            let (mut xp, mut xm) = (x, x);
            xp[j] += h;
            xm[j] -= h;
            let sp = law.stress(&wave.at(&xp[..2], t).1);
            let sm = law.stress(&wave.at(&xm[..2], t).1);
            div += (sp[i][j] - sm[i][j]) / (2.0 * h);
        }
        assert!(
            (law.rho * acc - div).abs() < 1e-6,
            "component {i}: ρü = {} vs div σ = {div}",
            law.rho * acc
        );
    }
    for s in [0.13, 0.5, 0.88] {
        for (j, p) in [(0, [0.0, s]), (0, [1.0, s]), (1, [s, 0.0]), (1, [s, 1.0])] {
            let (u, g) = wave.at(&p, t);
            let sig = law.stress(&g);
            assert!(u[j].abs() < 1e-15, "normal displacement {} at {p:?}", u[j]);
            assert!(
                sig[0][1].abs() < 1e-15,
                "shear stress {} at {p:?}",
                sig[0][1]
            );
        }
    }
}

/// Every generated (distorted) mesh fills the unit cube: positive Jacobians
/// at every point of the rule and total measure 1; the boundary flags mark
/// exactly the nodes on the faces.
#[test]
fn generated_meshes_fill_the_unit_cube() {
    for ty in ALL_TYPES {
        for n in [1, 3] {
            let sm = structured_mesh(ty, n, true);
            let rd = RefData::new(ty, M);
            let mut dndx = vec![0.0; sm.nn() * sm.dim];
            let mut vol = 0.0;
            for e in 0..sm.nb_elements() {
                for q in 0..rd.nq() {
                    vol += point_data(&sm, &rd, e, q, &mut dndx).1;
                }
            }
            assert!((vol - 1.0).abs() < 1e-12, "{ty} n={n}: measure {vol}");
            for node in 0..sm.nb_nodes() {
                let x = &sm.nodes[node * sm.dim..(node + 1) * sm.dim];
                let on_face = x
                    .iter()
                    .any(|&c| c.abs() < 1e-14 || (c - 1.0).abs() < 1e-14);
                assert_eq!(
                    on_face,
                    sm.on_boundary(node),
                    "{ty} n={n}: node {node} at {x:?}"
                );
            }
        }
    }
}

/// Patch test on the distorted meshes: a linear displacement field with
/// zero load is reproduced to solver precision by every type (the
/// consistency the convergence orders rest on).
#[test]
fn distorted_meshes_pass_the_linear_patch_test() {
    for ty in ALL_TYPES {
        let dim = ty.spatial_dimension();
        let law = law(dim, false);
        let sm = structured_mesh(ty, 2, true);
        let lin = |x: &[Real]| -> [Real; 3] {
            let mut u = [0.0; 3];
            for (i, ui) in u.iter_mut().enumerate().take(dim) {
                *ui = 1e-3 * (i as Real + 1.0);
                for (j, xj) in x.iter().enumerate() {
                    *ui += 1e-2 * ((i * 3 + j) as Real + 1.0) * xj;
                }
            }
            u
        };
        let u = static_solve(&sm, &law, &lin, &vec![0.0; sm.nb_nodes() * dim]);
        let mut worst = 0.0 as Real;
        for node in 0..sm.nb_nodes() {
            let ue = lin(&sm.nodes[node * dim..(node + 1) * dim]);
            for i in 0..dim {
                worst = worst.max((u[node * dim + i] - ue[i]).abs());
            }
        }
        assert!(worst < 1e-12, "{ty}: patch test error {worst:e}");
    }
}
