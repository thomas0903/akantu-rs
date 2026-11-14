// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Spatial convergence
// study (no upstream counterpart): observed orders of convergence of the diffusion model
// (src/model/diffusion_model/diffusion_model.cc, heat_diffusion.cc, the
// element classes of src/fe_engine/element_classes/* and the Gauss rules of
// gauss_integration.cc) under uniform refinement.

//! Spatial convergence of the heat-transfer model
//! (`docs/NUMERICAL_VALIDATION.md`, rule CONV-1 as amended). Generic harness:
//! `crates/akantu-fe/tests/convergence_common/mod.rs` (shared with the
//! solid, phase-field and coupler studies).
//!
//! * **Static** conduction `−div(κ ∇T) = s`, every regular element type
//!   (12) with an isotropic conductivity and the ten 2-D / 3-D types with an
//!   anisotropic one (full symmetric positive definite `κ`): the
//!   manufactured field `T = Π_j sin(k_j x_j + c_j)` with Dirichlet data
//!   `T` on the whole boundary and the consistent load of
//!   `s = −Σ κ_ij ∂_i ∂_j T`, on the distorted meshes of the solid study;
//!   errors in L2 (expected order `p + 1`) and in the energy norm
//!   `(∫ ∇e · κ ∇e)^½` (expected `p`).
//! * **Transient** (explicit, forward Euler with the lumped capacity, the
//!   model's `_explicit_lumped_mass` scheme): the decaying mode
//!   `T = e^{−(κ/ρc)π²(1 + 4)t} sin(πx) sin(2πy)` of the unit square with
//!   `T = 0` on the boundary, density, capacity and conductivity different
//!   from 1 and from each other (`ρ = 0.8`, `c = 2`, `κ = 1.3`: a misuse
//!   of the capacity changes the decay rate), `Δt ∝ h²`
//!   at a fixed mesh Fourier number `κΔt/(ρc h²) = 0.08`: L2 error second
//!   order in `h` (and first order in `Δt`, i.e. second order in `h` at the
//!   fixed ratio), energy error first order.
//!
//! The conductivity matrix and the internal flow are the library's (default
//! Gauss rules); the load vector, the geometry of the error integrals and
//! the errors use the test's own rules and derivatives of `N` by central
//! differences, so a defect of the library's rules, `dN/dξ` or of the
//! conductivity tensor changes the measured rates. Band (rule CONV-1):
//! `|r − r_th| ≤ 1/N` for every pair of levels `n → 2n`, `N = n·p`. The
//! finest 3-D level and the finest transient level run only in optimised
//! builds (`cargo test --release
//! -p akantu-diffusion --test spatial_convergence`, a line of
//! `ci/tools/feature-gates.txt`).

#[path = "../../akantu-fe/tests/convergence_common/mod.rs"]
mod convergence_common;

use std::f64::consts::PI;
use std::time::Instant;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Real};
use akantu_diffusion::HeatTransferModel;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use convergence_common::*;

use ElementType as E;

/// Points per direction of the test's own rule (exact for total degree 7
/// on every reference shape).
const M: usize = 5;

/// Refinement levels (cells per direction), as in the solid study: 1-D and
/// 2-D from `N = n·p = 8`; 3-D from `n = 2`, the finest 3-D level (the pair
/// with band 1/8) only in optimised builds.
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

/* ---------------------------------------------------------------------- */
/* Conductivity and manufactured field                                     */
/* ---------------------------------------------------------------------- */

/// A constant conductivity tensor (symmetric positive definite).
#[derive(Clone, Copy, Debug)]
struct Conductivity {
    dim: usize,
    k: [[Real; 3]; 3],
}

impl Conductivity {
    fn isotropic(dim: usize) -> Self {
        Self::scalar(dim, 1.0)
    }
    /// `κ I`.
    fn scalar(dim: usize, kappa: Real) -> Self {
        let mut k = [[0.0; 3]; 3];
        for (i, row) in k.iter_mut().enumerate().take(dim) {
            row[i] = kappa;
        }
        Self { dim, k }
    }
    /// A full tensor (eigenvalues 2-D: 2.36, 0.64; 3-D: 2.62, 1.31, 0.57,
    /// all positive).
    fn anisotropic(dim: usize) -> Self {
        let k = match dim {
            2 => [[2.0, 0.7, 0.0], [0.7, 1.0, 0.0], [0.0; 3]],
            3 => [[2.0, 0.6, 0.3], [0.6, 1.5, 0.5], [0.3, 0.5, 1.0]],
            _ => panic!("no anisotropic conductivity in {dim}-D"),
        };
        Self { dim, k }
    }
    fn is_isotropic(&self) -> bool {
        (0..self.dim).all(|i| (0..self.dim).all(|j| i == j || self.k[i][j] == 0.0))
    }
    /// Input file of the law with density `rho` and capacity `c`.
    fn dat(&self, rho: Real, c: Real) -> String {
        let rows: Vec<String> = (0..self.dim)
            .map(|i| {
                let r: Vec<String> = (0..self.dim).map(|j| format!("{}", self.k[i][j])).collect();
                format!("[{}]", r.join(", "))
            })
            .collect();
        format!(
            "model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    name = conv\n    \
             density = {rho}\n    capacity = {c}\n    conductivity = [{}]\n  ]\n]\n",
            rows.join(", ")
        )
    }
    /// `∇v · κ ∇v` of the gradient row `g[0]`.
    fn energy(&self, g: &[[Real; 3]; 3]) -> Real {
        let d = self.dim;
        let mut w = 0.0;
        for i in 0..d {
            for j in 0..d {
                w += g[0][i] * self.k[i][j] * g[0][j];
            }
        }
        w
    }
}

/// `T(x) = Π_j sin(k_j x_j + c_j)`: smooth, not polynomial, non-zero on
/// the boundary, with mixed second derivatives (seen by the off-diagonal
/// conductivities).
#[derive(Clone, Copy, Debug)]
struct Manufactured {
    dim: usize,
    kappa: Conductivity,
}

const K: [Real; 3] = [1.1, 0.7, 0.9];
const PHASE: [Real; 3] = [0.3, 0.5, 0.2];

impl Manufactured {
    /// `∂^o T` with `o[j]` derivatives along `x_j`.
    fn deriv(&self, o: [u32; 3], x: &[Real]) -> Real {
        let mut v = 1.0;
        for j in 0..self.dim {
            let k = K[j] * PI;
            v *= k.powi(o[j] as i32) * (k * x[j] + PHASE[j] + o[j] as Real * PI / 2.0).sin();
        }
        v
    }
    fn value(&self, x: &[Real]) -> ([Real; 3], [[Real; 3]; 3]) {
        let mut g = [[0.0; 3]; 3];
        for j in 0..self.dim {
            let mut o = [0; 3];
            o[j] = 1;
            g[0][j] = self.deriv(o, x);
        }
        ([self.deriv([0; 3], x), 0.0, 0.0], g)
    }
    /// `s = −div(κ ∇T) = −Σ_ij κ_ij ∂_i ∂_j T`.
    fn source(&self, x: &[Real]) -> [Real; 3] {
        let mut s = 0.0;
        for i in 0..self.dim {
            for j in 0..self.dim {
                let mut o = [0; 3];
                o[i] += 1;
                o[j] += 1;
                s -= self.kappa.k[i][j] * self.deriv(o, x);
            }
        }
        [s, 0.0, 0.0]
    }
}

/// Heat-transfer model on `sm` with conductivity `kappa`, density `rho`
/// and capacity `c`.
fn model_on(
    sm: &StructuredMesh,
    kappa: &Conductivity,
    [rho, c]: [Real; 2],
    method: AnalysisMethod,
) -> HeatTransferModel {
    let parser = Parser::parse_str(&kappa.dat(rho, c), "heat.dat").unwrap();
    let mut model = HeatTransferModel::with_parser(
        SharedMesh::new(sm.to_mesh()),
        None,
        "heat_transfer",
        parser,
    )
    .unwrap();
    model.init_full(&ModelOptions::new(method)).unwrap();
    model
}

/* ---------------------------------------------------------------------- */
/* Static manufactured solution                                            */
/* ---------------------------------------------------------------------- */

/// Static solve on `sm`: Dirichlet data `t_bc` on the whole boundary, nodal
/// external heat rate `load`; returns the nodal temperature.
fn static_solve(
    sm: &StructuredMesh,
    kappa: &Conductivity,
    t_bc: &dyn Fn(&[Real]) -> Real,
    load: &[Real],
) -> Vec<Real> {
    let dim = sm.dim;
    // a static solve does not use the capacity: `ρ = c = 1`
    let mut model = model_on(sm, kappa, [1.0, 1.0], AnalysisMethod::Static);
    let mut blocked = vec![false; sm.nb_nodes()];
    let mut t0 = vec![0.0; sm.nb_nodes()];
    for node in 0..sm.nb_nodes() {
        if sm.on_boundary(node) {
            blocked[node] = true;
            t0[node] = t_bc(&sm.nodes[node * dim..(node + 1) * dim]);
        }
    }
    model
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked);
    model
        .temperature_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&t0);
    model
        .external_heat_rate_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(load);
    model.solve_step(None).unwrap();
    model.temperature().unwrap().as_slice().to_vec()
}

fn static_level(ty: ElementType, n: usize, kappa: &Conductivity) -> Level {
    let t0 = Instant::now();
    let dim = ty.spatial_dimension();
    let sm = structured_mesh(ty, n, true);
    let exact = Manufactured { dim, kappa: *kappa };
    let rd = RefData::new(ty, M);
    let load = field_load(&sm, &rd, 1, &|x| exact.source(x));
    let t = static_solve(&sm, kappa, &|x| exact.value(x).0[0], &load);
    let [l2, energy, l2u, eu] = field_errors(&sm, &rd, &t, 1, &|x| exact.value(x), &|_, _, g| {
        kappa.energy(g)
    });
    Level {
        n,
        unknowns: sm.nb_nodes(),
        l2,
        energy,
        l2_rel: l2 / l2u,
        energy_rel: energy / eu,
        seconds: t0.elapsed().as_secs_f64(),
    }
}

fn static_study(ty: ElementType, anisotropic: bool) {
    let dim = ty.spatial_dimension();
    let kappa = if anisotropic {
        Conductivity::anisotropic(dim)
    } else {
        Conductivity::isotropic(dim)
    };
    let res: Vec<Level> = levels(ty)
        .iter()
        .map(|&n| static_level(ty, n, &kappa))
        .collect();
    let p = order(ty) as Real;
    let tag = if anisotropic {
        "static-anisotropic"
    } else {
        "static-isotropic"
    };
    let bad = report(tag, ty, &res, [p + 1.0, p], [EVERY_PAIR; 2]);
    assert!(bad.is_empty(), "rule CONV-1 violated:\n{}", bad.join("\n"));
}

macro_rules! static_tests {
    ($($name:ident: $ty:expr, $aniso:expr;)*) => {
        $(
            #[test]
            fn $name() {
                static_study($ty, $aniso);
            }
        )*
    };
}

static_tests! {
    static_isotropic_segment_2: E::Segment2, false;
    static_isotropic_segment_3: E::Segment3, false;
    static_isotropic_triangle_3: E::Triangle3, false;
    static_isotropic_triangle_6: E::Triangle6, false;
    static_isotropic_quadrangle_4: E::Quadrangle4, false;
    static_isotropic_quadrangle_8: E::Quadrangle8, false;
    static_isotropic_tetrahedron_4: E::Tetrahedron4, false;
    static_isotropic_tetrahedron_10: E::Tetrahedron10, false;
    static_isotropic_pentahedron_6: E::Pentahedron6, false;
    static_isotropic_pentahedron_15: E::Pentahedron15, false;
    static_isotropic_hexahedron_8: E::Hexahedron8, false;
    static_isotropic_hexahedron_20: E::Hexahedron20, false;
    static_anisotropic_triangle_3: E::Triangle3, true;
    static_anisotropic_triangle_6: E::Triangle6, true;
    static_anisotropic_quadrangle_4: E::Quadrangle4, true;
    static_anisotropic_quadrangle_8: E::Quadrangle8, true;
    static_anisotropic_tetrahedron_4: E::Tetrahedron4, true;
    static_anisotropic_tetrahedron_10: E::Tetrahedron10, true;
    static_anisotropic_pentahedron_6: E::Pentahedron6, true;
    static_anisotropic_pentahedron_15: E::Pentahedron15, true;
    static_anisotropic_hexahedron_8: E::Hexahedron8, true;
    static_anisotropic_hexahedron_20: E::Hexahedron20, true;
}

/* ---------------------------------------------------------------------- */
/* Transient: explicit forward Euler, lumped capacity                      */
/* ---------------------------------------------------------------------- */

/// Decaying mode of the unit square with `T = 0` on the boundary, the
/// solution of `ρc T_t = κ ΔT`: `T = e^{−(κ/ρc)|k|²t} sin(m₁πx)
/// sin(m₂πy)`, `|k|² = π²(m₁² + m₂²)`.
struct DecayingMode {
    /// Density `ρ`.
    rho: Real,
    /// Specific capacity `c`.
    capacity: Real,
    /// Isotropic conductivity `κ`.
    kappa: Real,
    m: [Real; 2],
}

/// The transient study's mode: `ρ`, `c` and `κ` different from 1 and from
/// each other (`ρc = 1.6`, `κ/ρc = 0.8125`), so that the capacity enters
/// the measured error (with `ρ = c = 1` a capacity
/// `ρ` instead of `ρc` passed the study).
const MODE: DecayingMode = DecayingMode {
    rho: 0.8,
    capacity: 2.0,
    kappa: 1.3,
    m: [1.0, 2.0],
};

impl DecayingMode {
    /// Diffusivity `κ/(ρc)`.
    fn diffusivity(&self) -> Real {
        self.kappa / (self.rho * self.capacity)
    }
    fn rate(&self) -> Real {
        self.diffusivity() * PI * PI * (self.m[0] * self.m[0] + self.m[1] * self.m[1])
    }
    /// End time of the transient study: `0.02 ρc/κ`, so that the mode
    /// decays to `e^{−5π²·0.02} = e^{−0.99}` and `n²/4` steps give the mesh
    /// Fourier number [`FOURIER`].
    fn t_end(&self) -> Real {
        0.02 / self.diffusivity()
    }
    fn at(&self, x: &[Real], t: Real) -> ([Real; 3], [[Real; 3]; 3]) {
        let a = (-self.rate() * t).exp();
        let (k0, k1) = (self.m[0] * PI, self.m[1] * PI);
        let (s0, c0) = ((k0 * x[0]).sin(), (k0 * x[0]).cos());
        let (s1, c1) = ((k1 * x[1]).sin(), (k1 * x[1]).cos());
        let mut g = [[0.0; 3]; 3];
        g[0][0] = a * k0 * c0 * s1;
        g[0][1] = a * k1 * s0 * c1;
        ([a * s0 * s1, 0.0, 0.0], g)
    }
}

/// Mesh Fourier number `κΔt/(ρc h²)` of the transient study (`h = 1/n`).
const FOURIER: Real = 0.08;

/// Forward Euler with the lumped capacity (`_explicit_lumped_mass`) from
/// the nodal interpolant of `T(·, 0)` and the consistent initial rate
/// (`set_consistent_initial_rate`), to `t_end` in `n²/4` steps
/// (`Δt = FOURIER·ρc h²/κ`); errors are the maxima over `t_end/4,
/// t_end/2, 3t_end/4, t_end`. Returns the level and `(Δt, stable Δt of the
/// model)`.
fn transient_level(ty: ElementType, n: usize, mode: &DecayingMode) -> (Level, [Real; 2]) {
    let t0 = Instant::now();
    let dim = ty.spatial_dimension();
    let sm = structured_mesh(ty, n, true);
    let rd = RefData::new(ty, M);
    let kappa = Conductivity::scalar(dim, mode.kappa);
    let mut model = model_on(
        &sm,
        &kappa,
        [mode.rho, mode.capacity],
        AnalysisMethod::ExplicitLumpedMass,
    );
    let nb = sm.nb_nodes();
    let mut blocked = vec![false; nb];
    let mut temp = vec![0.0; nb];
    for node in 0..nb {
        blocked[node] = sm.on_boundary(node);
        temp[node] = if blocked[node] {
            0.0
        } else {
            mode.at(&sm.nodes[node * dim..(node + 1) * dim], 0.0).0[0]
        };
    }
    model
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked);
    model
        .temperature_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&temp);
    let steps = n * n / 4;
    assert_eq!(steps % 4, 0);
    let dt = mode.t_end() / steps as Real;
    assert!((mode.diffusivity() * dt * (n * n) as Real - FOURIER).abs() < 1e-12);
    let dt_stable = model.stable_time_step().unwrap();
    model.set_time_step(dt, None).unwrap();
    model.set_consistent_initial_rate().unwrap();
    let (mut l2, mut energy, mut l2u, mut eu) =
        (0.0 as Real, 0.0 as Real, 0.0 as Real, 0.0 as Real);
    for s in 1..=steps {
        model.solve_step(None).unwrap();
        if (4 * s) % steps == 0 {
            let t = s as Real * dt;
            let u = model.temperature().unwrap().as_slice().to_vec();
            let [e0, e1, n0, n1] = field_errors(&sm, &rd, &u, 1, &|x| mode.at(x, t), &|_, _, g| {
                kappa.energy(g)
            });
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

/// Levels of the transient study: the finest (`n = 64`, 1024 steps,
/// ≈ 14 s unoptimised) only in optimised builds; the default profile keeps
/// the pairs with bands 1/8 and 1/16.
fn transient_levels() -> Vec<usize> {
    if cfg!(debug_assertions) {
        vec![8, 16, 32]
    } else {
        vec![8, 16, 32, 64]
    }
}

fn transient_study(ty: ElementType) {
    let mode = MODE;
    let mut res = Vec::new();
    for n in transient_levels() {
        let (level, [dt, stable]) = transient_level(ty, n, &mode);
        eprintln!(
            "transient {ty} n={n}: ρ = {}, c = {}, κ = {}, Δt = {dt:.4e}, model stable Δt = {stable:.4e} (ratio {:.3}), κΔt/(ρc h²) = {FOURIER}",
            mode.rho,
            mode.capacity,
            mode.kappa,
            dt / stable
        );
        assert!(
            dt < stable,
            "{ty} n={n}: Δt {dt} above the stable time step {stable}"
        );
        res.push(level);
    }
    let bad = report("transient", ty, &res, [2.0, 1.0], [EVERY_PAIR; 2]);
    assert!(bad.is_empty(), "rule CONV-1 violated:\n{}", bad.join("\n"));
}

#[test]
fn transient_explicit_quadrangle_4() {
    transient_study(E::Quadrangle4);
}

#[test]
fn transient_explicit_triangle_3() {
    transient_study(E::Triangle3);
}

/* ---------------------------------------------------------------------- */
/* Checks of the test's own data                                           */
/* ---------------------------------------------------------------------- */

/// `s = −div(κ ∇T)` of the manufactured field against central differences
/// of the flux `κ∇T` (step `h = 1e-4`: truncation `h²/6 · Σ|κ_ij| ·
/// max|∂⁴T| ≤ 1e-8/6 · 7.3 · (1.1π)⁴ ≈ 1.7e-6`, rounding ≈ 1e-11).
#[test]
fn source_is_minus_div_flux() {
    for (dim, aniso) in [(1, false), (2, false), (2, true), (3, false), (3, true)] {
        let kappa = if aniso {
            Conductivity::anisotropic(dim)
        } else {
            Conductivity::isotropic(dim)
        };
        let ex = Manufactured { dim, kappa };
        let x = [0.31, 0.67, 0.43];
        let s = ex.source(&x[..dim])[0];
        let h = 1e-4;
        let mut div = 0.0;
        for i in 0..dim {
            let (mut xp, mut xm) = (x, x);
            xp[i] += h;
            xm[i] -= h;
            let (_, gp) = ex.value(&xp[..dim]);
            let (_, gm) = ex.value(&xm[..dim]);
            for j in 0..dim {
                div += kappa.k[i][j] * (gp[0][j] - gm[0][j]) / (2.0 * h);
            }
        }
        assert!(
            (s + div).abs() < 2e-6,
            "dim {dim} aniso {aniso}: s = {s} vs −div(κ∇T) = {}",
            -div
        );
        assert_eq!(kappa.is_isotropic(), !aniso);
    }
}

/// The decaying mode solves `ρc T_t = κ ΔT` (central differences, time
/// step `1e-6`: truncation `≈ ρc · 1e-12/6 · (5(κ/ρc)π²)³ ≈ 2e-8`; space
/// step `1e-4`: truncation `≈ 2κ · 1e-8/12 · (2π)⁴ ≈ 3e-6`) and vanishes on
/// the boundary of the unit square; `ρ`, `c`, `κ` differ from 1 and from
/// each other.
#[test]
fn decaying_mode_is_an_exact_solution() {
    let mode = MODE;
    let [rho, c, kappa] = [mode.rho, mode.capacity, mode.kappa];
    assert!(rho != 1.0 && c != 1.0 && kappa != 1.0);
    assert!(rho != c && c != kappa && kappa != rho);
    let (x, t, h, ht) = ([0.37, 0.61], 0.0071, 1e-4, 1e-6);
    let tt = (mode.at(&x, t + ht).0[0] - mode.at(&x, t - ht).0[0]) / (2.0 * ht);
    let mut lap = 0.0;
    for j in 0..2 {
        let (mut xp, mut xm) = (x, x);
        xp[j] += h;
        xm[j] -= h;
        lap += (mode.at(&xp, t).0[0] - 2.0 * mode.at(&x, t).0[0] + mode.at(&xm, t).0[0]) / (h * h);
    }
    assert!(
        (rho * c * tt - kappa * lap).abs() < 1e-5,
        "ρc T_t = {} vs κΔT = {}",
        rho * c * tt,
        kappa * lap
    );
    for s in [0.13, 0.5, 0.88] {
        for p in [[0.0, s], [1.0, s], [s, 0.0], [s, 1.0]] {
            let v = mode.at(&p, t).0[0];
            assert!(v.abs() < 1e-15, "T = {v} at {p:?}");
        }
    }
}

/// Patch test on the distorted meshes with the anisotropic conductivity: a
/// linear temperature with zero source is reproduced to solver precision
/// by every type. (It cannot see a defect of the constant conductivity
/// itself: for a linear field the flux is constant and its discrete
/// divergence vanishes whatever `κ` is; the convergence study does.)
#[test]
fn distorted_meshes_pass_the_linear_patch_test() {
    for ty in ALL_TYPES {
        let dim = ty.spatial_dimension();
        let kappa = if dim == 1 {
            Conductivity::isotropic(1)
        } else {
            Conductivity::anisotropic(dim)
        };
        let sm = structured_mesh(ty, 2, true);
        let lin = |x: &[Real]| -> Real {
            let mut t = 0.3;
            for (j, xj) in x.iter().enumerate() {
                t += 0.1 * (j as Real + 1.0) * xj;
            }
            t
        };
        let t = static_solve(&sm, &kappa, &lin, &vec![0.0; sm.nb_nodes()]);
        let worst = (0..sm.nb_nodes()).fold(0.0 as Real, |w, node| {
            w.max((t[node] - lin(&sm.nodes[node * dim..(node + 1) * dim])).abs())
        });
        assert!(worst < 1e-12, "{ty}: patch test error {worst:e}");
    }
}
