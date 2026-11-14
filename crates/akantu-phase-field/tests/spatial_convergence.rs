// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Spatial convergence
// study (no upstream counterpart): observed orders of convergence of the phase-field model
// (src/model/phase_field/phase_field_model.cc, phasefield.cc,
// phasefields/phasefield_quadratic.cc and phasefield_linear.cc, the element
// classes of src/fe_engine/element_classes/* and the Gauss rules of
// gauss_integration.cc) under uniform refinement at a fixed length scale.

//! Spatial convergence of the phase-field model's damage equation at fixed
//! `l0` (`docs/NUMERICAL_VALIDATION.md`, rule CONV-1 as amended). Generic harness:
//! `crates/akantu-fe/tests/convergence_common/mod.rs`.
//!
//! For a prescribed strain field the damage equation of the `history` mode
//! is linear (`docs` of `akantu_phase_field::phasefield`):
//!
//! * AT2 (`quadratic`): `−div(g_c l0 ∇d) + (g_c/l0 + 2φ) d = 2φ`;
//! * AT1 (`linear`): `−div(¾ g_c l0 ∇d) + 2φ d = 2φ − 3g_c/(8 l0)`,
//!
//! with `φ = ½ ε : C : ε` the driving energy density of the strain the
//! coupler would write (`no_split`). The study takes a smooth damage field
//! `d* = a + b Π_j sin(k_j x_j + c_j) ∈ (0, 1)` with every argument in
//! `(0, π)` (so `Δd* = −|k|² (d* − a) ≤ 0`), solves each equation for the
//! driving term, `φ* = g_c (d*/l0 − l0 Δd*) / (2(1 − d*))` (AT2) and
//! `φ* = g_c (3/(8 l0) − ¾ l0 Δd*) / (2(1 − d*))` (AT1), both positive and
//! the AT1 one above the AT1 threshold `3g_c/(16 l0)` (the initial history
//! value), and prescribes at every quadrature point of the library the
//! strain `ε = s e₁⊗e₁` with `½ E s² = φ*(x_q)` (`ν = 0`). Then `d*` is the
//! exact solution of the model's equation with Dirichlet data `d*` on the
//! whole boundary and no external force: the driving term enters through
//! the law and the library's quadrature, the errors are measured with the
//! test's own rule. Orders: L2 `p + 1`, energy norm of each law's operator
//! (`(∫ g_c l0 |∇e|² + (g_c/l0 + 2φ*) e²)^½`, AT1 `(∫ ¾ g_c l0 |∇e|² +
//! 2φ* e²)^½`) `p`; every regular element type (12), both laws, `g_c = 1`,
//! `l0 = 0.5`, meshes with `h ≤ l0/2` in 1-D / 2-D and from `h = l0` in
//! 3-D (the pair `2 → 4` of `_hexahedron_20` reported only, [`asserted`]).
//! The 3-D levels of the pairs with a band ≤ 1/8 run only in optimised
//! builds (`cargo test --release -p akantu-phase-field --test
//! spatial_convergence`, a line of `ci/tools/feature-gates.txt`).
//!
//! The study covers the `history` mode only: the `penalization` mode
//! (penalties on a damage decrease and, AT1, on a negative damage) and the
//! `tao` mode (bounds from an external bound-constrained solver) are not
//! covered.

#[path = "../../akantu-fe/tests/convergence_common/mod.rs"]
mod convergence_common;

use std::f64::consts::PI;
use std::time::Instant;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{Array, ElementType, GhostType, Real};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_phase_field::PhaseFieldModel;
use convergence_common::*;

use ElementType as E;

/// Points per direction of the test's own rule.
const M: usize = 5;

/// Critical energy release rate and length scale of the study.
const GC: Real = 1.0;
const L0: Real = 0.5;
/// Young's modulus (`ν = 0`: `φ = ½ E ε:ε` for any strain).
const YOUNG: Real = 1.0;

/// Refinement levels, as in the solid and diffusion studies: 1-D and 2-D
/// from `N = n·p = 8` (`h ≤ l0/2`); 3-D from `n = 2`. The 3-D levels that
/// give the pairs with a band ≤ 1/8 run only in optimised builds (`n = 16`
/// for the first-order types, `n ≥ 8` for the second-order ones: `n = 8`
/// costs 6–12 s unoptimised per study). `_hexahedron_20` also has `n = 16`
/// (its pair `4 → 8` alone had `d·N` 0.92 / 0.90 of the
/// constant of CONV-1; ≈ 12 s per law optimised, 18,785 nodes). Its pair
/// `2 → 4` is reported only ([`asserted`]).
fn levels(ty: ElementType) -> Vec<usize> {
    let all = match (ty.spatial_dimension(), order(ty)) {
        (1 | 2, 1) => vec![8, 16, 32, 64],
        (1 | 2, _) => vec![4, 8, 16, 32],
        (_, 1) => vec![2, 4, 8, 16],
        _ if ty == E::Hexahedron20 => vec![2, 4, 8, 16],
        _ => vec![2, 4, 8],
    };
    if ty.spatial_dimension() == 3 && cfg!(debug_assertions) {
        let finest_unoptimised = if order(ty) == 1 { 8 } else { 4 };
        all.into_iter()
            .filter(|&n| n <= finest_unoptimised)
            .collect()
    } else {
        all
    }
}

/// Pairs asserted by rule CONV-1: every pair, except the pair `2 → 4` of
/// `_hexahedron_20`, which is reported only (CONV-1 as amended,
/// point 2: a failing pair is a finding; it is resolved as
/// pre-asymptotic only if the next finer pair halves its deviation (the
/// `1/N` law), no finer pair's `d·N` exceeds its own and the finest
/// asserted pair is within its band, and it stays in the output). At `h = l0` that pair breaks the band 0.25 (optimised run,
/// measured: AT2 L2 3.248 / energy 2.406, AT1 3.260 / 2.396; the
/// AT2 L2 order is inside by 0.002, the pair is reported only as a whole).
/// The cause is not the reaction term: the reaction part is 6–8 % of the
/// squared energy error at `n = 2`, and the diffusion model, which has no
/// reaction term, gives the same excess for the same field on the same
/// meshes (energy 2.386, L2 3.223). It is a pre-asymptotic
/// effect of this field on the 20-node serendipity hexahedra: the energy
/// deviation goes 0.406 → 0.115 → 0.020 over the pairs from `n = 2`, 4
/// and 8 (AT2; AT1 0.396 → 0.112 → 0.020), and every 3-D pair from 8 to
/// 16 is within 0.022 of its order (measured). An earlier
/// version had instead moved the 3-D levels of all six types to start
/// at `n = 4` after this failure, although the other five types pass
/// `2 → 4` (`docs/NUMERICAL_VALIDATION.md`).
fn asserted(ty: ElementType) -> Asserted {
    if ty == E::Hexahedron20 {
        Some(4)
    } else {
        EVERY_PAIR
    }
}

/// The two laws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Law {
    /// `quadratic`.
    At2,
    /// `linear`.
    At1,
}

impl Law {
    fn name(self) -> &'static str {
        match self {
            Law::At2 => "quadratic",
            Law::At1 => "linear",
        }
    }
    fn dat(self) -> String {
        format!(
            "phasefield {} [\n  name = conv\n  E = {YOUNG}\n  nu = 0\n  gc = {GC}\n  l0 = {L0}\n]\n",
            self.name()
        )
    }
    /// Coefficient of `|∇d|²` in the operator (`damage_energy`).
    fn gradient_coefficient(self) -> Real {
        match self {
            Law::At2 => GC * L0,
            Law::At1 => 0.75 * GC * L0,
        }
    }
    /// Reaction coefficient (`damage_energy_density`) for the driving term
    /// `φ`.
    fn reaction(self, phi: Real) -> Real {
        match self {
            Law::At2 => GC / L0 + 2.0 * phi,
            Law::At1 => 2.0 * phi,
        }
    }
    /// The driving term for which `d` (value) with Laplacian `lap` solves
    /// the law's equation pointwise.
    fn driving_term(self, d: Real, lap: Real) -> Real {
        match self {
            Law::At2 => GC * (d / L0 - L0 * lap) / (2.0 * (1.0 - d)),
            Law::At1 => GC * (3.0 / (8.0 * L0) - 0.75 * L0 * lap) / (2.0 * (1.0 - d)),
        }
    }
    /// Residual of the law's equation `−c Δd + r(φ) d − f(φ)` (zero for the
    /// manufactured pair).
    fn equation(self, d: Real, lap: Real, phi: Real) -> Real {
        let rhs = match self {
            Law::At2 => 2.0 * phi,
            Law::At1 => 2.0 * phi - 3.0 * GC / (8.0 * L0),
        };
        -self.gradient_coefficient() * lap + self.reaction(phi) * d - rhs
    }
}

/// `d* = a + b Π_j sin(k_j x_j + c_j)` with `k_j + c_j < π`, `c_j > 0`:
/// every factor in `[0.29, 1]` on the unit cube, `d* ∈ (0.2, 0.7]`.
#[derive(Clone, Copy, Debug)]
struct Damage {
    dim: usize,
}

const A: Real = 0.2;
const B: Real = 0.5;
const K: [Real; 3] = [0.8, 0.6, 0.7];
const PHASE: [Real; 3] = [0.3, 0.5, 0.4];

impl Damage {
    /// `Π_j sin(k_j x_j + c_j)` and its gradient.
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
    fn k2(&self) -> Real {
        (0..self.dim).map(|j| (K[j] * PI).powi(2)).sum()
    }
    /// `(d*, ∇d*)` in the layout of [`ExactField`].
    fn value(&self, x: &[Real]) -> ([Real; 3], [[Real; 3]; 3]) {
        let (s, g) = self.product(x);
        let mut grad = [[0.0; 3]; 3];
        for j in 0..self.dim {
            grad[0][j] = B * g[j];
        }
        ([A + B * s, 0.0, 0.0], grad)
    }
    /// `Δd* = −|k|² b Π sin`.
    fn laplacian(&self, x: &[Real]) -> Real {
        -self.k2() * B * self.product(x).0
    }
    /// The driving term `φ*(x)` of `law`.
    fn phi(&self, law: Law, x: &[Real]) -> Real {
        law.driving_term(self.value(x).0[0], self.laplacian(x))
    }
}

/// Strain `ε = s e₁⊗e₁` (column-major `dim × dim`) with `½ E s² = φ`.
fn strain_for(dim: usize, phi: Real) -> Vec<Real> {
    assert!(phi >= 0.0, "negative driving term {phi}");
    let mut eps = vec![0.0; dim * dim];
    eps[0] = (2.0 * phi / YOUNG).sqrt();
    eps
}

/// Phase-field model of `law` on `sm`, initialised for a static solve, with
/// the strain of the driving term `phi(x)` at every quadrature point.
fn model_on(sm: &StructuredMesh, law: Law, phi: &dyn Fn(&[Real]) -> Real) -> PhaseFieldModel {
    let parser = Parser::parse_str(&law.dat(), "material.dat").unwrap();
    let mut model = PhaseFieldModel::with_parser(
        SharedMesh::new(sm.to_mesh()),
        None,
        "phase_field_model",
        parser,
    )
    .unwrap();
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let dim = sm.dim;
    let mesh_h = model.mesh_handle().clone();
    let mesh = mesh_h.read().unwrap();
    assert_eq!(model.nb_phase_fields(), 1);
    let types = model
        .phase_field(0)
        .unwrap()
        .base()
        .filter_types(GhostType::NotGhost);
    for ty in types {
        let filter = model
            .phase_field(0)
            .unwrap()
            .base()
            .filter(ty, GhostType::NotGhost)
            .to_vec();
        let mut coords = Array::new(dim);
        model
            .fe_engine()
            .unwrap()
            .integration_points_coordinates(
                &mesh,
                &mut coords,
                ty,
                GhostType::NotGhost,
                Some(&filter),
            )
            .unwrap();
        let pf = model.phase_field_mut(0).unwrap();
        let mut s = pf
            .base_mut()
            .strain
            .get_mut(ty, GhostType::NotGhost)
            .unwrap();
        for (eps, x) in s
            .as_mut_slice()
            .chunks_exact_mut(dim * dim)
            .zip(coords.as_slice().chunks_exact(dim))
        {
            eps.copy_from_slice(&strain_for(dim, phi(x)));
        }
    }
    drop(mesh);
    model
}

/// Static solve: Dirichlet data `d_bc` on the whole boundary, no external
/// force; returns the nodal damage.
fn solve(
    model: &mut PhaseFieldModel,
    sm: &StructuredMesh,
    d_bc: &dyn Fn(&[Real]) -> Real,
) -> Vec<Real> {
    let dim = sm.dim;
    let mut blocked = vec![false; sm.nb_nodes()];
    let mut d0 = vec![0.0; sm.nb_nodes()];
    for node in 0..sm.nb_nodes() {
        if sm.on_boundary(node) {
            blocked[node] = true;
            d0[node] = d_bc(&sm.nodes[node * dim..(node + 1) * dim]);
        }
    }
    model
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked);
    model
        .damage_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&d0);
    model.solve_step(None).unwrap();
    model.damage().unwrap().as_slice().to_vec()
}

fn level(ty: ElementType, n: usize, law: Law) -> Level {
    let t0 = Instant::now();
    let dim = ty.spatial_dimension();
    let sm = structured_mesh(ty, n, true);
    let dam = Damage { dim };
    let rd = RefData::new(ty, M);
    let mut model = model_on(&sm, law, &|x| dam.phi(law, x));
    let d = solve(&mut model, &sm, &|x| dam.value(x).0[0]);
    let c = law.gradient_coefficient();
    let [l2, energy, l2u, eu] = field_errors(&sm, &rd, &d, 1, &|x| dam.value(x), &|x, v, g| {
        let grad: Real = (0..dim).map(|j| g[0][j] * g[0][j]).sum();
        c * grad + law.reaction(dam.phi(law, x)) * v[0] * v[0]
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

fn study(ty: ElementType, law: Law) {
    let res: Vec<Level> = levels(ty).iter().map(|&n| level(ty, n, law)).collect();
    let p = order(ty) as Real;
    let tag = match law {
        Law::At2 => "AT2",
        Law::At1 => "AT1",
    };
    let bad = report(tag, ty, &res, [p + 1.0, p], [asserted(ty); 2]);
    assert!(bad.is_empty(), "rule CONV-1 violated:\n{}", bad.join("\n"));
}

macro_rules! studies {
    ($($name:ident: $ty:expr, $law:expr;)*) => {
        $(
            #[test]
            fn $name() {
                study($ty, $law);
            }
        )*
    };
}

studies! {
    at2_segment_2: E::Segment2, Law::At2;
    at2_segment_3: E::Segment3, Law::At2;
    at2_triangle_3: E::Triangle3, Law::At2;
    at2_triangle_6: E::Triangle6, Law::At2;
    at2_quadrangle_4: E::Quadrangle4, Law::At2;
    at2_quadrangle_8: E::Quadrangle8, Law::At2;
    at2_tetrahedron_4: E::Tetrahedron4, Law::At2;
    at2_tetrahedron_10: E::Tetrahedron10, Law::At2;
    at2_pentahedron_6: E::Pentahedron6, Law::At2;
    at2_pentahedron_15: E::Pentahedron15, Law::At2;
    at2_hexahedron_8: E::Hexahedron8, Law::At2;
    at2_hexahedron_20: E::Hexahedron20, Law::At2;
    at1_segment_2: E::Segment2, Law::At1;
    at1_segment_3: E::Segment3, Law::At1;
    at1_triangle_3: E::Triangle3, Law::At1;
    at1_triangle_6: E::Triangle6, Law::At1;
    at1_quadrangle_4: E::Quadrangle4, Law::At1;
    at1_quadrangle_8: E::Quadrangle8, Law::At1;
    at1_tetrahedron_4: E::Tetrahedron4, Law::At1;
    at1_tetrahedron_10: E::Tetrahedron10, Law::At1;
    at1_pentahedron_6: E::Pentahedron6, Law::At1;
    at1_pentahedron_15: E::Pentahedron15, Law::At1;
    at1_hexahedron_8: E::Hexahedron8, Law::At1;
    at1_hexahedron_20: E::Hexahedron20, Law::At1;
}

/* ---------------------------------------------------------------------- */
/* Checks of the test's own data                                           */
/* ---------------------------------------------------------------------- */

/// The manufactured pair solves each law's equation (Laplacian by central
/// differences, step 1e-4: truncation `≈ 2 · 1e-8/12 · |k|⁴ ≈ 1e-7`), stays
/// in `(0, 1)`, and its driving term is positive (AT1: above the threshold
/// `3g_c/(16 l0)` of the history initial value) at sample points of the
/// unit cube.
#[test]
fn manufactured_damage_solves_the_equations() {
    for dim in 1..=3 {
        let dam = Damage { dim };
        for law in [Law::At2, Law::At1] {
            for t in [0.0, 0.17, 0.5, 0.83, 1.0] {
                let x = [t, 1.0 - t, 0.5 * t + 0.25];
                let x = &x[..dim];
                let d = dam.value(x).0[0];
                assert!(d > 0.2 && d < 0.71, "d* = {d} at {x:?}");
                let h = 1e-4;
                let mut lap = 0.0;
                for j in 0..dim {
                    let (mut xp, mut xm) = (x.to_vec(), x.to_vec());
                    xp[j] += h;
                    xm[j] -= h;
                    lap += (dam.value(&xp).0[0] - 2.0 * d + dam.value(&xm).0[0]) / (h * h);
                }
                assert!(
                    (lap - dam.laplacian(x)).abs() < 1e-6,
                    "dim {dim}: Δd* = {} vs FD {lap}",
                    dam.laplacian(x)
                );
                let phi = dam.phi(law, x);
                assert!(phi > 0.0, "{law:?}: φ* = {phi}");
                if law == Law::At1 {
                    assert!(
                        phi > 3.0 * GC / (16.0 * L0),
                        "AT1 φ* = {phi} below the threshold"
                    );
                }
                let r = law.equation(d, lap, phi);
                assert!(r.abs() < 1e-6, "{law:?} dim {dim}: residual {r} at {x:?}");
            }
        }
    }
}

/// The prescribed strain gives the law exactly the manufactured driving
/// term: after a solve the law's `phi` internal equals `φ*(x_q)` at every
/// quadrature point (relative 1e-14).
#[test]
fn prescribed_strain_gives_the_manufactured_driving_term() {
    for ty in [E::Segment3, E::Triangle6, E::Hexahedron8] {
        for law in [Law::At2, Law::At1] {
            let dim = ty.spatial_dimension();
            let sm = structured_mesh(ty, 2, true);
            let dam = Damage { dim };
            let mut model = model_on(&sm, law, &|x| dam.phi(law, x));
            solve(&mut model, &sm, &|x| dam.value(x).0[0]);
            let mesh_h = model.mesh_handle().clone();
            let mesh = mesh_h.read().unwrap();
            let mut coords = Array::new(dim);
            model
                .fe_engine()
                .unwrap()
                .integration_points_coordinates(&mesh, &mut coords, ty, GhostType::NotGhost, None)
                .unwrap();
            let pf = model.phase_field(0).unwrap();
            let phi = pf.base().phi.get(ty, GhostType::NotGhost).unwrap();
            for (p, x) in phi
                .as_slice()
                .iter()
                .zip(coords.as_slice().chunks_exact(dim))
            {
                let want = dam.phi(law, x);
                assert!(
                    (p - want).abs() <= 1e-14 * want,
                    "{ty} {law:?}: φ = {p} vs φ* = {want}"
                );
            }
        }
    }
}

/// Patch test on the distorted meshes, both laws: a linear damage
/// `d = 0.3 + Σ 0.1 (j + 1) x_j` with its driving term (`Δd = 0`) is
/// reproduced to solver precision by every type (it cannot see the
/// coefficient of the gradient term: for a linear field the gradient flux
/// is constant and its discrete divergence vanishes; the convergence study
/// does).
#[test]
fn distorted_meshes_pass_the_linear_patch_test() {
    for ty in ALL_TYPES {
        for law in [Law::At2, Law::At1] {
            let dim = ty.spatial_dimension();
            let sm = structured_mesh(ty, 2, true);
            let lin = |x: &[Real]| -> Real {
                let mut d = 0.3;
                for (j, xj) in x.iter().enumerate() {
                    d += 0.1 * (j as Real + 1.0) * xj;
                }
                d
            };
            let mut model = model_on(&sm, law, &|x| law.driving_term(lin(x), 0.0));
            let d = solve(&mut model, &sm, &lin);
            let worst = (0..sm.nb_nodes()).fold(0.0 as Real, |w, node| {
                w.max((d[node] - lin(&sm.nodes[node * dim..(node + 1) * dim])).abs())
            });
            assert!(worst < 1e-12, "{ty} {law:?}: patch test error {worst:e}");
        }
    }
}
