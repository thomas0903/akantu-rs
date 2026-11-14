// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_solid_mechanics_model_igfem/
// test_solid_mechanics_model_igfem.cc (+ .verified, material.dat, plate.msh).
// Built only with the feature `cgal` (GPL-3.0-or-later sphere intersector,
// decision D-042).

//! TEST-200 and the circular-inclusion convergence study (D-007).
//!
//! The problem (T.-P. Fries, "A corrected XFEM approximation without
//! problems in blending elements", 2008): a disk of radius `R = 2` with a
//! circular inclusion of radius `a` (inside: `gel`, `E = 1`, `ν = 0.25`;
//! outside: `aggregate`, `E = 10`, `ν = 0.3`, plane strain), loaded by the
//! radial displacement `u_r(R) = R`. The exact solution is radial:
//! `u_r = [(1 − R²/a²) α + R²/a²] r` for `r ≤ a`, `u_r = (r − R²/r) α +
//! R²/r` for `r ≥ a`, with `α = (λ₁ + μ₁ + μ₂) R² / ((λ₂ + μ₂) a² + (λ₁ +
//! μ₁)(R² − a²) + μ₂ R²)` (1 = inclusion, 2 = matrix). Square meshes inside
//! the disk get the exact displacement on their boundary.

mod common;

use akantu_core::{Element, ElementKind, ElementType, GhostType, Real, TypeFilter};
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::SolidMechanicsModelIgfem;
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::{ConstitutiveLawSelector, SelectorContext};
use common::model::*;

const DAT: &str = include_str!("data/upstream/smm_igfem/material.dat");
const OUTER_RADIUS: Real = 2.0;

/// Upstream `SphereMaterialSelector`: IGFEM elements → the IGFEM law;
/// regular elements whose barycenter lies in a sphere (`|x − c| < r +
/// 1e-12`) → law 1 (`gel`), else law 0 (`aggregate`).
#[derive(Debug)]
struct SphereMaterialSelector {
    igfem: usize,
    center: [Real; 2],
    radius: Real,
}

impl ConstitutiveLawSelector for SphereMaterialSelector {
    fn select(&self, ctx: &SelectorContext<'_>, el: Element) -> akantu_core::Result<Option<usize>> {
        if el.ty().kind() == ElementKind::Igfem {
            return Ok(Some(self.igfem));
        }
        let b = ctx.mesh.barycenter(el)?;
        let d = ((b[0] - self.center[0]).powi(2) + (b[1] - self.center[1]).powi(2)).sqrt();
        Ok(Some(usize::from(d < self.radius + 1e-12)))
    }
}

/// `(λ, μ)` of the laws `aggregate` (0) and `gel` (1).
fn lame(model: &SolidMechanicsModelIgfem) -> [(Real, Real); 2] {
    let get = |i: usize, p: &str| match model.smm.material(i).unwrap().get_param(p).unwrap() {
        akantu_parser::ParamValue::Real(v) => v,
        v => panic!("{p}: {v:?}"),
    };
    [
        (get(0, "lambda"), get(0, "mu")),
        (get(1, "lambda"), get(1, "mu")),
    ]
}

/// Upstream `computeAlpha(inner, outer, lambda, mu)` (index 0 = matrix,
/// 1 = inclusion).
fn alpha(a: Real, r_out: Real, l: [(Real, Real); 2]) -> Real {
    let ((l0, m0), (l1, m1)) = (l[0], l[1]);
    (l1 + m1 + m0) * r_out * r_out
        / ((l0 + m0) * a * a + (l1 + m1) * (r_out * r_out - a * a) + (m0 * r_out * r_out))
}

/// Exact displacement at `x`.
fn exact(x: [Real; 2], a: Real, alpha: Real) -> [Real; 2] {
    let r = (x[0] * x[0] + x[1] * x[1]).sqrt();
    if r == 0.0 {
        return [0.0, 0.0];
    }
    let r2 = OUTER_RADIUS * OUTER_RADIUS;
    let ur = if r <= a {
        ((1. - r2 / (a * a)) * alpha + r2 / (a * a)) * r
    } else {
        (r - r2 / r) * alpha + r2 / r
    };
    [ur * x[0] / r, ur * x[1] / r]
}

/// Upstream `applyBoundaryConditions`: every node on the bounding box
/// (`eps = 1e-12 · height`) gets the exact matrix-side displacement
/// `cos φ ((r − 4/r) α + 4/r)`, `sin φ (…)` (upstream formula with
/// `R² = 4`).
fn apply_bcs(model: &mut SolidMechanicsModelIgfem, alpha: Real) {
    let pos = model.mesh().read().unwrap().nodes().clone();
    let (mut lo, mut hi) = ([Real::INFINITY; 2], [Real::NEG_INFINITY; 2]);
    for x in pos.tuples() {
        for d in 0..2 {
            lo[d] = lo[d].min(x[d]);
            hi[d] = hi[d].max(x[d]);
        }
    }
    let eps = ((hi[1] - lo[1]) * 1e-12).abs();
    let mut disp = model.smm.displacement().unwrap().clone();
    let mut boun = model.smm.blocked_dofs().unwrap().clone();
    disp.fill(0.0);
    boun.fill(false);
    for i in 0..pos.len() {
        let x = pos.tuple(i);
        let on = (x[0] - lo[0]).abs() < eps
            || (x[0] - hi[0]).abs() < eps
            || (x[1] - hi[1]).abs() < eps
            || (x[1] - lo[1]).abs() < eps;
        if on {
            let radius = (x[0] * x[0] + x[1] * x[1]).sqrt();
            let phi = x[1].atan2(x[0]);
            let v = (radius - 4. / radius) * alpha + 4. / radius;
            boun.tuple_mut(i).copy_from_slice(&[true, true]);
            disp.tuple_mut(i)
                .copy_from_slice(&[phi.cos() * v, phi.sin() * v]);
        }
    }
    model
        .smm
        .displacement_mut()
        .unwrap()
        .copy_from(&disp)
        .unwrap();
    model
        .smm
        .blocked_dofs_mut()
        .unwrap()
        .copy_from(&boun)
        .unwrap();
}

/// Build, mesh the inclusion, solve; returns the model and `α`.
fn solve(mesh: Mesh, a: Real) -> (SolidMechanicsModelIgfem, Real) {
    solve_with(mesh, a, 2)
}

/// [`solve`] with `max_iterations` Newton iterations (threshold `1e-12`,
/// increment criterion, as upstream).
fn solve_with(mesh: Mesh, a: Real, max_iterations: i64) -> (SolidMechanicsModelIgfem, Real) {
    let mut model = model(mesh, DAT);
    model.set_material_selector(Box::new(SphereMaterialSelector {
        igfem: 2,
        center: [0.0, 0.0],
        radius: a,
    }));
    init_static(&mut model);
    model
        .register_geometry_object(
            vec![SkSphere::try_from_f64([0.0, 0.0, 0.0], a * a).unwrap()],
            "gel",
        )
        .unwrap();
    model.update("gel").unwrap();
    let al = alpha(a, OUTER_RADIUS, lame(&model));
    apply_bcs(&mut model, al);
    set_newton(&mut model, max_iterations, 1e-12);
    model.smm.solve_step(None).unwrap();
    (model, al)
}

/// `test_solid_mechanics_model_igfem.cc`: `a = 0.401` on the 4 × 4 plate;
/// the printed absolute displacements of the 29 nodes (25 standard, 4
/// enriched; `std::fixed`, 6 decimals) equal the `.verified` output.
#[test]
fn test_solid_mechanics_model_igfem() {
    let mesh = common::read_msh(2, "smm_igfem/plate.msh", "mesh");
    let (model, _) = solve(mesh, 0.401);
    let u = model.smm.displacement().unwrap();
    let verified = std::fs::read_to_string(common::upstream(
        "smm_igfem/test_solid_mechanics_model_igfem.verified",
    ))
    .unwrap();
    let lines: Vec<&str> = verified.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(lines.len(), u.len(), "number of nodes");
    for (n, line) in lines.iter().enumerate() {
        let mine = format!("{:.6},{:.6}", u.tuple(n)[0].abs(), u.tuple(n)[1].abs());
        assert_eq!(&mine, line, "node {n}: {:?}", u.tuple(n));
    }
}

/// Degree-5 7-point rule on the reference triangle (Radon): barycentric
/// points and weights summing to 1.
fn radon7() -> Vec<([Real; 3], Real)> {
    let s15 = (15.0 as Real).sqrt();
    let (a1, a2) = ((6. - s15) / 21., (6. + s15) / 21.);
    let (w1, w2) = ((155. + s15) / 1200., (155. - s15) / 1200.);
    let mut v = vec![([1. / 3., 1. / 3., 1. / 3.], 9. / 40.)];
    for (a, w) in [(a1, w1), (a2, w2)] {
        let b = 1. - 2. * a;
        v.push(([a, a, b], w));
        v.push(([a, b, a], w));
        v.push(([b, a, a], w));
    }
    v
}

/// `‖u_h − u‖_{L²} / ‖u‖_{L²}` with the 7-point rule on every triangle and
/// on the two triangles of every quadrangle sub-element (the FE field is
/// evaluated at the physical points: barycentric interpolation on regular
/// triangles, `IgfemFeEngine::interpolate` on IGFEM elements).
fn l2_error_exact(model: &SolidMechanicsModelIgfem, a: Real, alpha: Real) -> Real {
    let mesh_h = model.mesh().clone();
    let mesh = mesh_h.read().unwrap();
    let u = model.smm.displacement().unwrap();
    let engine = model.ctx.engine().unwrap();
    let rule = radon7();
    let (mut err, mut nrm) = (0.0, 0.0);
    let mut tri = |p: [[Real; 2]; 3], eval: &dyn Fn([Real; 2]) -> [Real; 2]| {
        let area = 0.5
            * ((p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]));
        for (l, w) in &rule {
            let x = [
                l[0] * p[0][0] + l[1] * p[1][0] + l[2] * p[2][0],
                l[0] * p[0][1] + l[1] * p[1][1] + l[2] * p[2][1],
            ];
            let (uh, ue) = (eval(x), exact(x, a, alpha));
            err += w * area * ((uh[0] - ue[0]).powi(2) + (uh[1] - ue[1]).powi(2));
            nrm += w * area * (ue[0].powi(2) + ue[1].powi(2));
        }
    };
    for ty in mesh
        .element_types(TypeFilter {
            spatial_dimension: Some(2),
            ghost_type: GhostType::NotGhost,
            kind: None,
        })
        .collect::<Vec<_>>()
    {
        for e in 0..mesh.nb_element(ty, GhostType::NotGhost) {
            let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
            let x = mesh
                .extract_nodal_values_from_element(&mesh.nodes(), el)
                .unwrap();
            let ue = mesh.extract_nodal_values_from_element(&u, el).unwrap();
            let p = |k: usize| [x[2 * k], x[2 * k + 1]];
            if ty == ElementType::Triangle3 {
                let pts = [p(0), p(1), p(2)];
                let eval = |q: [Real; 2]| {
                    let l = common::cut::barycentric(pts, q);
                    [
                        l[0] * ue[0] + l[1] * ue[2] + l[2] * ue[4],
                        l[0] * ue[1] + l[1] * ue[3] + l[2] * ue[5],
                    ]
                };
                tri(pts, &eval);
                continue;
            }
            let eval = |q: [Real; 2]| {
                let v = engine.interpolate(&mesh, &q, &ue, el).unwrap();
                [v[0], v[1]]
            };
            for s in 0..2 {
                let c: Vec<usize> = akantu_igfem::element::sub_element_connectivity(ty, s)
                    .unwrap()
                    .iter()
                    .map(|&k| usize::from(k))
                    .collect();
                if c.len() == 3 {
                    tri([p(c[0]), p(c[1]), p(c[2])], &eval);
                } else {
                    tri([p(c[0]), p(c[1]), p(c[2])], &eval);
                    tri([p(c[0]), p(c[2]), p(c[3])], &eval);
                }
            }
        }
    }
    (err / nrm).sqrt()
}

/// Structured `n × n` mesh of `[-4/3, 4/3]²` (the upstream plate's square).
fn plate(n: usize) -> Mesh {
    let (nodes, tris) = common::cut::structured(n, -4. / 3., 4. / 3., -4. / 3., 4. / 3.);
    let mut mesh = Mesh::new(2, "plate").unwrap();
    {
        let mut acc = akantu_mesh::MeshAccessor::new(&mut mesh);
        for p in &nodes {
            acc.push_node(p).unwrap();
        }
        let mut conn = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for t in &tris {
            conn.push_tuple(&t.map(|v| akantu_core::Idx::try_from(v).unwrap()));
        }
    }
    mesh
}

/// h-convergence of the IGFEM solution of the inclusion problem vs the
/// exact solution (L2 norm of the displacement error, relative, 7-point
/// rule on every triangle / sub-triangle). Observed (2026-11-11, aarch64,
/// debug and release identical): n = 8, 16, 32, 64 → 2.561e-2, 9.658e-3,
/// 3.450e-3, 9.421e-4 (rates 1.41, 1.49, 1.87); n = 128 (release, 3 Newton
/// iterations: the absolute increment check `1e-12` sees round-off of a
/// 33 000-DOF system) 2.583e-4 (rate 1.87). The coarse meshes are
/// pre-asymptotic (the inclusion radius spans 1.2 to 4.8 elements); the
/// asymptotic rate ≈ 1.87, close to the optimal 2 of linear elements
/// (upstream's mesh generation ignores edges that the circle crosses twice,
/// and the interface is the inscribed polygon). Criteria: monotone
/// decrease, rate of the two finest meshes ≥ 1.8, finest error < 1.5e-3
/// (`docs/NUMERICAL_VALIDATION.md`, PROPOSED).
#[test]
fn circular_inclusion_convergence() {
    let a = 0.401;
    let mut hist: Vec<(Real, Real)> = Vec::new();
    for n in [8, 16, 32, 64] {
        let (model, al) = solve(plate(n), a);
        let e = l2_error_exact(&model, a, al);
        let h = (8. / 3.) / n as Real;
        let c = counts(&model.mesh().read().unwrap());
        eprintln!("n = {n}: h = {h:.4}, L2 = {e:.4e}, counts {c:?}");
        hist.push((h, e));
    }
    for w in hist.windows(2) {
        assert!(w[1].1 < w[0].1, "{hist:?}");
    }
    let [.., (hp, ep), (h, e)] = hist[..] else {
        unreachable!()
    };
    let rate = (ep / e).ln() / (hp / h).ln();
    eprintln!("finest rate {rate:.3}");
    assert!(rate >= 1.8, "rate {rate}");
    assert!(e < 1.5e-3, "{e}");
}

/// Rigid-body modes through the model (independent check): a rigid motion
/// `u = t + ω × x` (small rotation, linearised) on the standard nodes with
/// zero enrichments produces no strain, hence zero internal forces at every
/// node, on the regular and the IGFEM elements. Tolerance `1e-13 · E_max ·
/// |u|_max · h`: nodal forces are sums of `Bᵀσ · J` with `σ` the rounding
/// residue of `∇u` (≈ ε |u|/h) times the moduli.
#[test]
fn rigid_body_modes_give_no_internal_force() {
    let a = 0.401;
    let mut model = model(plate(16), DAT);
    model.set_material_selector(Box::new(SphereMaterialSelector {
        igfem: 2,
        center: [0.0, 0.0],
        radius: a,
    }));
    init_static(&mut model);
    model
        .register_geometry_object(
            vec![SkSphere::try_from_f64([0.0, 0.0, 0.0], a * a).unwrap()],
            "gel",
        )
        .unwrap();
    model.update("gel").unwrap();
    let nodes = model.mesh().read().unwrap().nodes().clone();
    let nstd = 17 * 17;
    for (t, w) in [([0.3, -0.2], 0.0), ([0.0, 0.0], 1.0), ([1.0, 2.0], -0.5)] {
        {
            let mut u = model.smm.displacement_mut().unwrap();
            for n in 0..nodes.len() {
                let x = nodes.tuple(n);
                let v = if n < nstd {
                    [t[0] - w * x[1], t[1] + w * x[0]]
                } else {
                    [0.0, 0.0]
                };
                u.tuple_mut(n).copy_from_slice(&v);
            }
        }
        model.smm.assemble_internal_forces().unwrap();
        let f = model.smm.internal_force().unwrap();
        let fmax = f.as_slice().iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let tol = 1e-13 * 10.0 * 3.0 * (8. / 3. / 16.);
        assert!(
            fmax <= tol,
            "rigid mode t={t:?} w={w}: max |f_int| = {fmax:e}"
        );
    }
    assert!(
        model
            .mesh()
            .read()
            .unwrap()
            .nb_element(ElementType::IgfemTriangle5, GhostType::NotGhost)
            > 0
    );
}
